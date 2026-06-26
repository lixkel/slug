use git2;
use crate::errors::SlugError;


// Per branch history under refs/<prefix>/<branch>, outside refs/heads
const SHARED_PREFIX: &str = "refs/slug";        // intended to be pushed in CI
const LOCAL_PREFIX: &str = "refs/slug-local";   // never pushed, stays local
// Notes mapping evaluated commits to their slug data commit
const SHARED_NOTES_REF: &str = "refs/notes/slug-shared";
const LOCAL_NOTES_REF: &str = "refs/notes/slug-local";

pub struct SlugGit {
    pub repo: git2::Repository,
    pub slug_ref: String,
    pub notes_ref: String,
}


impl SlugGit {
    // Shared history handle for the current branch
    pub fn shared() -> Result<Self, SlugError> {
        Self::open(SHARED_PREFIX, SHARED_NOTES_REF)
    }

    // Local history handle for the current branch
    pub fn local() -> Result<Self, SlugError> {
        Self::open(LOCAL_PREFIX, LOCAL_NOTES_REF)
    }

    fn open(prefix: &str, notes_ref: &str) -> Result<Self, SlugError> {
        let repo = git2::Repository::discover(".")?;
        let branch = current_branch(&repo)?;
        let slug_ref = format!("{}/{}", prefix, branch);
        Ok(Self { repo, slug_ref, notes_ref: notes_ref.to_string() })
    }

    // Find closest slug record commit by following the current HEADs ancestors
    // None = no ancestor was benchmarked 
    pub fn resolve_base_commit(&self) -> Result<Option<git2::Commit<'_>>, SlugError> {
        let head = self.repo.head()?.peel_to_commit()?;
        // Start looking from HEAD parent we are not interested in other results for this commit
        let mut current = head.parent(0).ok();
        while let Some(commit) = current {
            if let Some(slug_oid) = self.check_notes(commit.id())? {
                return Ok(Some(self.repo.find_commit(slug_oid)?));
            }
            current = commit.parent(0).ok();
        }
        Ok(None)
    }

    // Find if commit with this oid (hash) have note pointing to its slug record commit
    fn check_notes(&self, oid: git2::Oid) -> Result<Option<git2::Oid>, SlugError> {
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
        match self.resolve_base_commit()? {
            Some(commit) => self.read_file_from(&commit, name),
            None => Ok(None),
        }
    }

    // Does the Slug record file for this test already exists
    pub fn base_file_exists(&self, name: &str) -> Result<bool, SlugError> {
        match self.resolve_base_commit()? {
            Some(commit) => Ok(commit.tree()?.get_name(name).is_some()),
            None => Ok(false),
        }
    }

    // Append the updates to Slug record files inherited form ancestors and point the branch ref at a new commit
    pub fn edit_branch_slug(&self, commit_hash: &str, updates: &[(String, String)]) -> Result<String, SlugError> {
        let base = self.resolve_base_commit()?;
        let base_tree = match &base {
            Some(commit) => Some(commit.tree()?),
            None => None,
        };

        let mut tree_builder = self.repo.treebuilder(base_tree.as_ref())?;

        for (test_name, test_data) in updates {
            let mut content = String::new();
            if let Some(tree) = &base_tree {
                if let Some(entry) = tree.get_name(test_name) {
                    let blob = self.repo.find_blob(entry.id())?;
                    content = String::from_utf8(blob.content().to_vec()).map_err(|e| SlugError::Parsing(e.to_string()))?;
                }
            }

            content.push_str(test_data);
            let content_oid = self.repo.blob(content.as_bytes())?;
            tree_builder.insert(test_name, content_oid, 0o100644)?;
        }

        let new_oid = tree_builder.write()?;
        let tree = self.repo.find_tree(new_oid)?;
        let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
        let message = format!("Benchmark data for {}\n\nTarget-Commit: {}", commit_hash, commit_hash);

        let parents: Vec<&git2::Commit> = base.iter().collect();
        let new_commit_oid = self.repo.commit(None, &sig, &sig, &message, &tree, &parents)?;

        // Force-move the branch ref; the base may live on another branch's chain
        self.repo.reference(&self.slug_ref, new_commit_oid, true, "slug record")?;

        Ok(new_commit_oid.to_string())
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

    // Returns latest Slug record commit from this branch
    fn branch_tip_commit(&self) -> Result<Option<git2::Commit<'_>>, SlugError> {
        match self.repo.find_reference(&self.slug_ref) {
            Ok(reference) => Ok(Some(reference.peel_to_commit()?)),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    // Read tests historical records from this commit
    fn read_file_from(&self, commit: &git2::Commit, name: &str) -> Result<Option<Vec<u8>>, SlugError> {
        let tree = commit.tree()?;
        let entry = match tree.get_name(name) {
            Some(entry) => entry,
            None => return Ok(None),
        };
        Ok(Some(self.repo.find_blob(entry.id())?.content().to_vec()))
    }

    // Full recorded history for every test on this branch
    // Each file in the tip commit tree already holds the tests whole history
    pub fn read_all_history(&self) -> Result<Vec<(String, String)>, SlugError> {
        let commit = match self.branch_tip_commit()? {
            Some(commit) => commit,
            None => return Ok(Vec::new()),
        };

        let mut out = Vec::new();
        for entry in commit.tree()?.iter() {
            let name = match entry.name() {
                Some(name) => name.to_string(),
                None => continue,
            };
            let blob = self.repo.find_blob(entry.id())?;
            let content = String::from_utf8(blob.content().to_vec()).map_err(|e| SlugError::Parsing(e.to_string()))?;
            out.push((name, content));
        }
        Ok(out)
    }

}

fn current_branch(repo: &git2::Repository) -> Result<String, SlugError> {
    let head = repo.head()?;
    // TODO: look at this:
    // Falls back to "HEAD" when there is no short name, which is the case for detached HEAD
    let name = head.shorthand().unwrap_or("HEAD");
    // Slashes flattened so it is a single ref segment (avoiding refs/slug/feature/foo)
    Ok(name.replace('/', "-"))
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

    // One data ref per branch under each prefix
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
