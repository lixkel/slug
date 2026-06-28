use git2;
use crate::errors::SlugError;


// History lines under refs/<prefix>/<tip-source-sha>, outside refs/heads
const SHARED_PREFIX: &str = "refs/slug";        // intended to be pushed in CI
const LOCAL_PREFIX: &str = "refs/slug-local";   // never pushed, stays local
// Notes mapping evaluated commits to their record
const SHARED_NOTES_REF: &str = "refs/notes/slug-shared";
const LOCAL_NOTES_REF: &str = "refs/notes/slug-local";

pub struct SlugGit {
    pub repo: git2::Repository,
    pub ref_prefix: String,
    pub notes_ref: String,
}


impl SlugGit {
    // Shared history handle
    pub fn shared() -> Result<Self, SlugError> {
        Self::open(SHARED_PREFIX, SHARED_NOTES_REF)
    }

    // Local history handle
    pub fn local() -> Result<Self, SlugError> {
        Self::open(LOCAL_PREFIX, LOCAL_NOTES_REF)
    }

    fn open(prefix: &str, notes_ref: &str) -> Result<Self, SlugError> {
        let repo = git2::Repository::discover(".")?;
        Ok(Self { repo, ref_prefix: prefix.to_string(), notes_ref: notes_ref.to_string() })
    }

    // Construct git reference name for passed commit hash
    fn tip_ref(&self, sha: &str) -> String {
        format!("{}/{}", self.ref_prefix, sha)
    }

    // Inverse of tip_ref, the source commit hash encoded in a reference name file
    fn tip_source(ref_name: &str) -> Option<git2::Oid> {
        ref_name.rsplit('/').next().and_then(|s| git2::Oid::from_str(s).ok())
    }

    // Delete reference for passed line, if exists
    fn delete_tip_ref(&self, slug_commit: &git2::Commit) -> Result<(), SlugError> {
        if let Some(source) = Self::record_source(slug_commit) {
            if let Ok(mut tip) = self.repo.find_reference(&self.tip_ref(&source)) {
                tip.delete()?;
            }
        }
        Ok(())
    }

    // Find all lines whose source commit descends from `commit_hash`
    fn descendant_lines(&self, commit_hash: &str) -> Result<Vec<String>, SlugError> {
        let ancestor = git2::Oid::from_str(commit_hash)?;
        let glob = format!("{}/*", self.ref_prefix);
        let names: Vec<String> = self.repo.references_glob(&glob)?
            .names()
            .filter_map(|name| name.ok().map(String::from))
            .collect();

        let mut lines = Vec::new();
        for name in names {
            let tip_source = match Self::tip_source(&name) {
                Some(oid) => oid,
                None => continue,
            };
            // Skip lines whose source commit is gone
            if self.repo.find_commit(tip_source).is_err() {
                continue;
            }
            if self.repo.graph_descendant_of(tip_source, ancestor)? {
                lines.push(name);
            }
        }
        Ok(lines)
    }

    // Find closest record by following the current HEADs ancestors
    // None = no ancestor was benchmarked
    pub fn resolve_base_record(&self) -> Result<Option<git2::Commit<'_>>, SlugError> {
        let head = self.repo.head()?.peel_to_commit()?;
        // Start looking from HEAD parent we are not interested in other results for this commit
        let mut current = head.parent(0).ok();
        while let Some(commit) = current {
            if let Some(slug_oid) = self.find_record(commit.id())? {
                return Ok(Some(self.repo.find_commit(slug_oid)?));
            }
            current = commit.parent(0).ok();
        }
        Ok(None)
    }

    // Find if commit with this oid (hash) has a note pointing to its record
    fn find_record(&self, oid: git2::Oid) -> Result<Option<git2::Oid>, SlugError> {
        match self.repo.find_note(Some(&self.notes_ref), oid) {
            Ok(note) => {
                let parsed = note.message()
                    .and_then(|m| m.trim().strip_prefix("Benchmark-Results: "))
                    .and_then(|s| git2::Oid::from_str(s.trim()).ok());
                Ok(parsed)
            }
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // Read the Slug record file for a test from ancestor commit
    pub fn read_base_file(&self, name: &str) -> Result<Option<Vec<u8>>, SlugError> {
        match self.resolve_base_record()? {
            Some(commit) => self.read_record_file(&commit, name),
            None => Ok(None),
        }
    }

    // Does the Slug record file for this test already exists
    pub fn base_file_exists(&self, name: &str) -> Result<bool, SlugError> {
        match self.resolve_base_record()? {
            Some(commit) => Ok(commit.tree()?.get_name(name).is_some()),
            None => Ok(false),
        }
    }

    // Record this commit's test results and slide it into its line at
    // position dictated by source ancestry of original commit, rewriting every benchmarked
    // descendant so the new row appears in its test file too
    // If record for this commit already exists we replace it
    // Returns (source_commit, data_commit) pairs for every commit written
    pub fn record_data(&self, commit_hash: &str, updates: &[(String, String)]) -> Result<Vec<(String, String)>, SlugError> {
        let base = self.resolve_base_record()?;

        // Base this commit's data on top of its nearest benchmarked ancestor
        let base_tree = match &base {
            Some(commit) => Some(commit.tree()?),
            None => None,
        };
        let new_oid = self.write_record(commit_hash, base.as_ref(), base_tree.as_ref(), updates)?;
        let mut notes = vec![(commit_hash.to_string(), new_oid.to_string())];

        // Lines whose tip descends from this commit need new
        // data spliced in, linear branch yields at most one
        // TODO: more than one is benchmarked source fork which is not supported
        let lines = self.descendant_lines(commit_hash)?;
        if lines.len() > 1 {
            return Err(SlugError::parsing("commit has multiple benchmarked descendant lines (merge/fork not supported)"));
        }

        match lines.first() {
            // No line descends from this commit, this commit becomes a new leaf of its line
            None => {
                self.repo.reference(&self.tip_ref(commit_hash), new_oid, true, "slug record")?;
                // If the old base was a leaf, this commit becomes anchor
                if let Some(base_commit) = &base {
                    self.delete_tip_ref(base_commit)?;
                }
            }
            // Splice into an existing line and replay its records above the base
            Some(line_ref) => {
                let base_oid = base.as_ref().map(git2::Commit::id);
                let tip = self.repo.find_reference(line_ref)?.peel_to_commit()?;

                let mut descendants = Vec::new();
                let mut current = Some(tip);
                while let Some(commit) = current {
                    if Some(commit.id()) == base_oid {
                        break;
                    }
                    let parent = commit.parent(0).ok();
                    // Skip an existing record for this same commit, it will be replaced
                    let already_recorded = Self::record_source(&commit).as_deref() == Some(commit_hash);
                    if !already_recorded {
                        descendants.push(commit);
                    }
                    current = parent;
                }
                descendants.reverse(); // oldest first

                let mut prev = self.repo.find_commit(new_oid)?;
                for descendant in &descendants {
                    let source = Self::record_source(descendant)
                        .ok_or_else(|| SlugError::parsing("record without Source-Commit"))?;
                    let rebuilt = self.replay_record(descendant, &source, &prev)?;
                    notes.push((source, rebuilt.to_string()));
                    prev = self.repo.find_commit(rebuilt)?;
                }

                // Change the line reference to the newly rebuilt tip
                self.repo.reference(line_ref, prev.id(), true, "slug record")?;
            }
        }

        Ok(notes)
    }

    // Seed a record's tree from `base_tree`, append each test's new rows and commit it parented on `parent`
    fn write_record(&self, commit_hash: &str, parent: Option<&git2::Commit>, base_tree: Option<&git2::Tree>, updates: &[(String, String)]) -> Result<git2::Oid, SlugError> {
        let mut tree_builder = self.repo.treebuilder(base_tree)?;
        for (test_name, test_data) in updates {
            let mut content = String::new();
            if let Some(tree) = base_tree {
                if let Some(entry) = tree.get_name(test_name) {
                    content = self.blob_string(entry.id())?;
                }
            }
            content.push_str(test_data);
            let content_oid = self.repo.blob(content.as_bytes())?;
            tree_builder.insert(test_name, content_oid, 0o100644)?;
        }
        let tree_oid = tree_builder.write()?;
        let tree = self.repo.find_tree(tree_oid)?;
        self.commit_record(commit_hash, &tree, parent)
    }

    // Rebuild `original` on top of `prev`, keep prev's tree and reappend only the rows `original` itself measured
    fn replay_record(&self, original: &git2::Commit, source: &str, prev: &git2::Commit) -> Result<git2::Oid, SlugError> {
        let prev_tree = prev.tree()?;
        let mut tree_builder = self.repo.treebuilder(Some(&prev_tree))?;
        // Loop through files in commit (records for each test)
        for entry in original.tree()?.iter() {
            let name = match entry.name() {
                Some(name) => name.to_string(),
                None => continue,
            };
            let original_content = self.blob_string(entry.id())?;
            let own = Self::own_rows(&original_content, source);
            if own.is_empty() {
                continue; // this test is untouched in this record
            }
            let new_content = match prev_tree.get_name(&name) {
                Some(prev_entry) => {
                    let mut content = self.blob_string(prev_entry.id())?;
                    for row in &own {
                        content.push_str(row);
                        content.push('\n');
                    }
                    content
                }
                // this test didn't exist in `prev`
                None => original_content,
            };
            let content_oid = self.repo.blob(new_content.as_bytes())?;
            tree_builder.insert(&name, content_oid, 0o100644)?;
        }
        let tree_oid = tree_builder.write()?;
        let tree = self.repo.find_tree(tree_oid)?;
        self.commit_record(source, &tree, Some(prev))
    }

    // Commit a record tree for `commit_hash`, optionally parented.
    fn commit_record(&self, commit_hash: &str, tree: &git2::Tree, parent: Option<&git2::Commit>) -> Result<git2::Oid, SlugError> {
        let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
        let message = format!("Benchmark data for {}\n\nSource-Commit: {}", commit_hash, commit_hash);
        let parents: Vec<&git2::Commit> = parent.into_iter().collect();
        Ok(self.repo.commit(None, &sig, &sig, &message, tree, &parents)?)
    }

    pub fn add_note(&self, target_commit_hash: &str, note_message: &str) -> Result<(), SlugError> {
        let oid = git2::Oid::from_str(target_commit_hash)?;
        let sig = git2::Signature::now("Slug", "slug@slug.internal")?;

        self.repo.note(
            &sig,
            &sig,
            Some(&self.notes_ref),
            oid,
            note_message,
            true // Overwrite if note already exists
        )?;

        Ok(())
    }

    // Call Git housekeeping to pack loose objects, libgit2 never calls gc
    // Each record commit writes a whole file blob per record (more when replay
    // rewrites descendants) so loose objects grow until packed
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

    // Find closest record for HEAD
    // Returns HEAD's own record if it exists, else its nearest benchmarked ancestor's, None if nothing recorded
    fn head_record(&self) -> Result<Option<git2::Commit<'_>>, SlugError> {
        let head = self.repo.head()?.peel_to_commit()?;
        if let Some(oid) = self.find_record(head.id())? {
            return Ok(Some(self.repo.find_commit(oid)?));
        }
        self.resolve_base_record()
    }

    // Decode a blob as UTF-8 text
    fn blob_string(&self, oid: git2::Oid) -> Result<String, SlugError> {
        let blob = self.repo.find_blob(oid)?;
        Ok(String::from_utf8(blob.content().to_vec())?)
    }

    // Read tests historical records from this commit
    fn read_record_file(&self, commit: &git2::Commit, name: &str) -> Result<Option<Vec<u8>>, SlugError> {
        let tree = commit.tree()?;
        let entry = match tree.get_name(name) {
            Some(entry) => entry,
            None => return Ok(None),
        };
        Ok(Some(self.repo.find_blob(entry.id())?.content().to_vec()))
    }

    // Full recorded history starting from HEAD
    pub fn read_all_history(&self) -> Result<Vec<(String, String)>, SlugError> {
        let commit = match self.head_record()? {
            Some(commit) => commit,
            None => return Ok(Vec::new()),
        };

        let mut out = Vec::new();
        for entry in commit.tree()?.iter() {
            let name = match entry.name() {
                Some(name) => name.to_string(),
                None => continue,
            };
            let content = self.blob_string(entry.id())?;
            out.push((name, content));
        }
        Ok(out)
    }

    // Determine source commit from record's Source-Commit trailer
    fn record_source(commit: &git2::Commit) -> Option<String> {
        commit.message()?
            .lines()
            .find_map(|line| line.strip_prefix("Source-Commit: "))
            .map(|s| s.trim().to_string())
    }

    // Data rows in record files associated with `source`
    fn own_rows(content: &str, source: &str) -> Vec<String> {
        content
            .lines()
            .filter(|line| line.rsplit(',').next().map(str::trim) == Some(source))
            .map(|line| line.to_string())
            .collect()
    }

}

// Returns HEAD's commit hash
pub fn get_commit_hash() -> Result<String, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

// Clean every trace after Slug, delete shared and local histories, plus notes
// Deleting a ref drops its commits from the graph and git garbage collects them
pub fn clean() -> Result<Vec<String>, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let mut removed = Vec::new();

    // One ref per line under each prefix
    for glob in [format!("{}/*", SHARED_PREFIX), format!("{}/*", LOCAL_PREFIX)] {
        let names: Vec<String> = repo.references_glob(&glob)?
            .names()
            .filter_map(|name| name.ok().map(String::from))
            .collect();
        for name in names {
            repo.find_reference(&name)?.delete()?;
            removed.push(name);
        }
    }

    for refname in [SHARED_NOTES_REF, LOCAL_NOTES_REF] {
        match repo.find_reference(refname) {
            Ok(mut reference) => {
                reference.delete()?;
                removed.push(refname.to_string());
            }
            // Missing, nothing to remove
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(removed)
}

// True if source commit is reachable from any branch tip
fn source_reachable(repo: &git2::Repository, branch_tips: &[git2::Oid], source: git2::Oid) -> bool {
    for &tip in branch_tips {
        if let Ok(base) = repo.merge_base(tip, source) {
            if base == source {
                return true;
            }
        }
    }
    false
}

// Bring tip refs back in line with the branches that still exist
// Returns removed ref names
pub fn prune() -> Result<Vec<String>, SlugError> {
    let repo = git2::Repository::discover(".")?;

    // Collect Git branch tips to test reachability against
    let mut branch_tips = Vec::new();
    for glob in ["refs/heads/*", "refs/remotes/origin/*"] {
        let names: Vec<String> = repo.references_glob(glob)?
            .names()
            .filter_map(|name| name.ok().map(String::from))
            .collect();
        for name in names {
            if let Ok(commit) = repo.find_reference(&name).and_then(|r| r.peel_to_commit()) {
                branch_tips.push(commit.id());
            }
        }
    }

    // Refuse to prune when no branches are visible (detached CI checkout):
    if branch_tips.is_empty() {
        return Err(SlugError::parsing("no branches found, refusing to prune"));
    }

    let mut removed = Vec::new();
    // Shared and local stores hold distinct commits so resolve each separately
    for prefix in [SHARED_PREFIX, LOCAL_PREFIX] {
        let glob = format!("{}/*", prefix);
        let line_names: Vec<String> = repo.references_glob(&glob)?
            .names()
            .filter_map(|name| name.ok().map(String::from))
            .collect();

        // For each line walk records from the tip down to the first one
        // whose source commit is still reachable, make that record the new tip
        let mut anchors: Vec<(git2::Oid, String)> = Vec::new();
        for name in &line_names {
            let tip = match repo.find_reference(name).and_then(|r| r.peel_to_commit()) {
                Ok(commit) => commit,
                Err(_) => continue,
            };
            let mut current = Some(tip);
            while let Some(record) = current {
                if let Some(source) = SlugGit::record_source(&record) {
                    if let Ok(oid) = git2::Oid::from_str(&source) {
                        if source_reachable(&repo, &branch_tips, oid) {
                            anchors.push((record.id(), source));
                            break;
                        }
                    }
                }
                current = record.parent(0).ok();
            }
        }

        // Keep only the maximal anchors 
        let mut desired: Vec<(String, git2::Oid)> = Vec::new();
        for (oid, source) in &anchors {
            if desired.iter().any(|(_, kept)| kept == oid) {
                continue; // same record reached from two lines
            }
            // if another anchor descends from this one, its ref will keep this record alive
            let subsumed = anchors.iter().any(|(other, _)| {
                other != oid && repo.graph_descendant_of(*other, *oid).unwrap_or(false)
            });
            if !subsumed {
                desired.push((format!("{}/{}", prefix, source), *oid));
            }
        }

        // Delete every existing ref that is not desired (unreachable)
        for name in &line_names {
            if !desired.iter().any(|(desired_name, _)| desired_name == name) {
                repo.find_reference(name)?.delete()?;
                removed.push(name.clone());
            }
        }
        // Create any desired ref that does not exist
        for (name, oid) in &desired {
            if repo.find_reference(name).is_err() {
                repo.reference(name, *oid, true, "slug prune")?;
            }
        }
    }
    Ok(removed)
}
