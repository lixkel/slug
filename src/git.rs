use git2;
use std::collections::HashMap;
use crate::errors::SlugError;


// Benchmark results live directly in a git note on the source commit
// Note's body holds the name of benchmark on first line followed by a self
// describing CSV consisting of header line and then one row per data point
// each row ending in source commit hash:
//
//   # BenchmarkFib-8
//   mean,commit_hash
//   12345,<source-sha>
//
// Benchmark's history for a commit is reconstructed on read by walking first
// parent ancestry and collecting each ancestors records
const SHARED_NOTES_REF: &str = "refs/notes/slug-shared";   // intended to be pushed in CI
const LOCAL_NOTES_REF: &str = "refs/notes/slug-local";     // never pushed, stays local

pub struct SlugGit {
    pub repo: git2::Repository,
    pub notes_ref: String,
}


impl SlugGit {
    // Shared history handle
    pub fn shared() -> Result<Self, SlugError> {
        Self::open(SHARED_NOTES_REF)
    }

    // Local history handle
    pub fn local() -> Result<Self, SlugError> {
        Self::open(LOCAL_NOTES_REF)
    }

    fn open(notes_ref: &str) -> Result<Self, SlugError> {
        let repo = git2::Repository::discover(".")?;
        Ok(Self { repo, notes_ref: notes_ref.to_string() })
    }

    // Read the Slug record note for this commit, None if it has none
    fn read_note(&self, oid: git2::Oid) -> Result<Option<String>, SlugError> {
        match self.repo.find_note(Some(&self.notes_ref), oid) {
            Ok(note) => Ok(note.message().ok().map(str::to_string)),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // Split the note body into ordered (test_name, lines) pairs
    fn parse_note(body: &str) -> Vec<(String, Vec<String>)> {
        let mut sections: Vec<(String, Vec<String>)> = Vec::new();
        let mut current: Option<(String, Vec<String>)> = None;
        for line in body.lines() {
            if let Some(name) = line.strip_prefix("# ") {
                if let Some(section) = current.take() {
                    sections.push(section);
                }
                current = Some((name.trim().to_string(), Vec::new()));
            } else if let Some((_, lines)) = current.as_mut() {
                if !line.is_empty() {
                    lines.push(line.to_string());
                }
            }
        }
        if let Some(section) = current.take() {
            sections.push(section);
        }
        sections
    }

    // Is this line Slug record note header
    fn is_header(line: &str) -> bool {
        line.rsplit(',').next().map(str::trim) == Some("commit_hash")
    }

    // Walk first parent ancestry from `start` (inclusive), collecting every commit's
    // records into per test pair (header, data-rows-oldest-first)
    // `only` restricts collection to a single test
    fn collect_history(&self, start: git2::Commit, only: Option<&str>) -> Result<HashMap<String, (Option<String>, Vec<String>)>, SlugError> {
        let mut acc: HashMap<String, (Option<String>, Vec<String>)> = HashMap::new();
        let mut current = Some(start);
        // Walk from newest to oldest
        while let Some(commit) = current {
            if let Some(body) = self.read_note(commit.id())? {
                for (name, lines) in Self::parse_note(&body) {
                    if only.is_some_and(|want| want != name) {
                        continue;
                    }
                    let entry = acc.entry(name).or_insert((None, Vec::new()));
                    for line in lines {
                        if Self::is_header(&line) {
                            if entry.0.is_none() {
                                entry.0 = Some(line);
                            }
                        } else {
                            entry.1.push(line);
                        }
                    }
                }
            }
            current = commit.parent(0).ok();
        }
        for (_, rows) in acc.values_mut() {
            rows.reverse(); // oldest first
        }
        Ok(acc)
    }

    // Assemble a reconstructed test file (header + rows) as CSV text, None without a header
    fn assemble(header: Option<&String>, rows: &[String]) -> Option<String> {
        let header = header?;
        let mut out = String::new();
        out.push_str(header);
        out.push('\n');
        for row in rows {
            out.push_str(row);
            out.push('\n');
        }
        Some(out)
    }

    // Reconstruct the history of benchmarks from HEAD's ancestry (HEAD excluded)
    pub fn read_base_file(&self, name: &str, limit: Option<usize>) -> Result<Option<String>, SlugError> {
        let head = self.repo.head()?.peel_to_commit()?;
        let start = match head.parent(0).ok() {
            Some(commit) => commit,
            None => return Ok(None),
        };

        let mut header: Option<String> = None;
        let mut rows: Vec<String> = Vec::new(); // newest first while walking history
        let mut current = Some(start);
        while let Some(commit) = current {
            if let Some(body) = self.read_note(commit.id())? {
                for (section, lines) in Self::parse_note(&body) {
                    if section != name {
                        continue;
                    }
                    for line in lines {
                        if Self::is_header(&line) {
                            header.get_or_insert(line);
                        } else {
                            rows.push(line);
                        }
                    }
                }
            }
            // Stop when the window is satisfied
            if let Some(n) = limit {
                if header.is_some() && rows.len() >= n {
                    break;
                }
            }
            current = commit.parent(0).ok();
        }

        rows.reverse(); // oldest first
        if let Some(n) = limit {
            if rows.len() > n {
                rows.drain(0..rows.len() - n);
            }
        }
        Ok(Self::assemble(header.as_ref(), &rows))
    }

    // Export full history from HEAD (included)
    pub fn read_all_history(&self) -> Result<Vec<(String, String)>, SlugError> {
        let head = self.repo.head()?.peel_to_commit()?;
        let acc = self.collect_history(head, None)?;

        let mut out = Vec::new();
        for (name, (header, rows)) in acc {
            if let Some(text) = Self::assemble(header.as_ref(), &rows) {
                out.push((name, text));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    // Record this commit's benchmark results in note, descendant commits
    // inherit this data through its ancestors
    pub fn record_data(&self, commit_hash: &str, updates: &[(String, String)]) -> Result<(), SlugError> {
        let mut body = String::new();
        for (name, csv) in updates {
            body.push_str("# ");
            body.push_str(name);
            body.push('\n');
            body.push_str(csv);
            if !csv.ends_with('\n') {
                body.push('\n');
            }
        }
        self.add_note(commit_hash, &body)
    }

    // Write or overwrite commit's Slug note
    fn add_note(&self, commit_hash: &str, note_body: &str) -> Result<(), SlugError> {
        let oid = git2::Oid::from_str(commit_hash)?;
        let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
        self.repo.note(&sig, &sig, Some(&self.notes_ref), oid, note_body, true)?;
        Ok(())
    }

    // Delete selected store's notes ref, removing all its Slug data
    // Returns false if there was nothing to delete
    pub fn clean(&self) -> Result<bool, SlugError> {
        match self.repo.find_reference(&self.notes_ref) {
            Ok(mut reference) => {
                reference.delete()?;
                Ok(true)
            }
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    // Call Git housekeeping to pack loose objects, libgit2 never calls gc
    // --auto runs housekeeping only when required, otherwise it early exits
    pub fn gc_auto(&self) -> Result<(), SlugError> {
        let status = std::process::Command::new("git")
            .arg("--git-dir")
            .arg(self.repo.path())
            .args(["gc", "--auto", "--quiet"])
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(SlugError::parsing(format!("git gc --auto exited with {}", status)))
        }
    }
}

// Returns HEAD's commit hash
pub fn get_commit_hash() -> Result<String, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

// True if `source` is reachable from any ref tip
fn source_reachable(repo: &git2::Repository, ref_tips: &[git2::Oid], source: git2::Oid) -> bool {
    for &tip in ref_tips {
        if let Ok(base) = repo.merge_base(tip, source) {
            if base == source {
                return true;
            }
        }
    }
    false
}

// Drop notes whose source commit is no longer reachable from any ref (branch, remote, or tag)
// Returns removed source commit hashes
pub fn prune() -> Result<Vec<String>, SlugError> {
    let repo = git2::Repository::discover(".")?;

    // Collect all refs that keep commits alive
    let mut ref_tips = Vec::new();
    for glob in ["refs/heads/*", "refs/remotes/*", "refs/tags/*"] {
        let names: Vec<String> = repo.references_glob(glob)?
            .names()
            .filter_map(|name| name.ok().map(String::from))
            .collect();
        for name in names {
            if let Ok(commit) = repo.find_reference(&name).and_then(|r| r.peel_to_commit()) {
                ref_tips.push(commit.id());
            }
        }
    }

    // Refuse to prune when no refs are visible (e.g. detached CI checkout)
    if ref_tips.is_empty() {
        return Err(SlugError::parsing("no refs found, refusing to prune"));
    }

    let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
    let mut removed = Vec::new();
    for notes_ref in [SHARED_NOTES_REF, LOCAL_NOTES_REF] {
        // Collect the marked commits first, we cannot delete while iterating
        let marked: Vec<git2::Oid> = match repo.notes(Some(notes_ref)) {
            Ok(notes) => notes.flatten().map(|(_, target)| target).collect(),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        for target in marked {
            if !source_reachable(&repo, &ref_tips, target) {
                repo.note_delete(target, Some(notes_ref), &sig, &sig)?;
                removed.push(target.to_string());
            }
        }
    }
    Ok(removed)
}
