use git2;
use std::path::Path;
use crate::errors::SlugError;


// Shared history, custom ref pushed in CI (outside refs/heads so it stays out of web branch UI)
const SHARED_REF: &str = "refs/slug/shared";
// Local history, ref outside refs/heads so it is never pushed
const LOCAL_REF: &str = "refs/slug-local/slug";
// Notes attached to evaluated commits, pointing to the slug data commit
const NOTES_REF: &str = "refs/notes/slug";

pub struct SlugGit {
    pub repo: git2::Repository,
    pub slug_ref: String,
}


impl SlugGit {
    // Shared history handle (creates the branch if missing)
    pub fn shared() -> Result<Self, SlugError> {
        let instance = Self::open(SHARED_REF)?;
        instance.ensure_ref_exists()?;
        Ok(instance)
    }

    // Local history handle (creates the ref if missing)
    pub fn local() -> Result<Self, SlugError> {
        let instance = Self::open(LOCAL_REF)?;
        instance.ensure_ref_exists()?;
        Ok(instance)
    }

    // Readonly handle to local history
    pub fn open_local() -> Result<Self, SlugError> {
        Self::open(LOCAL_REF)
    }

    // Readonly handle to the shared history
    pub fn open_shared() -> Result<Self, SlugError> {
        Self::open(SHARED_REF)
    }

    fn open(slug_ref: &str) -> Result<Self, SlugError> {
        let repo = git2::Repository::discover(".")?;
        Ok(Self { repo, slug_ref: slug_ref.to_string() })
    }

    pub fn ref_exists(&self) -> Result<bool, SlugError> {
        match self.repo.find_reference(&self.slug_ref) {
            Ok(_) => Ok(true),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub fn create_ref(&self) -> Result<(), SlugError> {
        let treebuilder = self.repo.treebuilder(None)?;
        let tree_oid = treebuilder.write()?;
        let tree = self.repo.find_tree(tree_oid)?;

        let sig = git2::Signature::now("Slug", "slug@slug.internal").map_err(|e| SlugError::Git(e))?;

        self.repo.commit(
            Some(&self.slug_ref),
            &sig,
            &sig,
            "Initial slug commit",
            &tree,
            &[],
        )?;

        Ok(())
    }

    pub fn ensure_ref_exists(&self) -> Result<(), SlugError> {
        if !self.ref_exists()? {
            self.create_ref()?;
        }
        Ok(())
    }

    pub fn edit_branch_slug(&self, commit_hash: &str, updates: &[(String, String)]) -> Result<String, SlugError> {
        let branch = self.repo.find_reference(&self.slug_ref)?;
        let branch_commit = branch.peel_to_commit()?;

        // Prepare TreeBuilder and write test_data to the test_name file
        let tree = branch_commit.tree()?;
        let mut tree_builder = self.repo.treebuilder(Some(&tree))?;

        for (test_name, test_data) in updates {
            let mut current_content = String::new();
            if self.file_exists(test_name)? {
                // Get the current content of the file
                let entry = tree.get_name(test_name).ok_or_else(|| SlugError::Git(git2::Error::from_str("File not found in tree")))?;
                let blob = self.repo.find_blob(entry.id())?;
                current_content = String::from_utf8(blob.content().to_vec()).map_err(|e| SlugError::Parsing(e.to_string()))?;
            }

            // Append new data to existing content
            current_content.push_str(test_data);

            let content_oid = self.repo.blob(current_content.as_bytes())?;
            tree_builder.insert(test_name, content_oid, 0o100644)?;
        }
        
        let updated_tree_oid = tree_builder.write()?;

        // Create a new commit on this updated tree and use the parent commit
        let sig = git2::Signature::now("Slug", "slug@slug.internal").map_err(|e| SlugError::Git(e))?;
        let updated_tree = self.repo.find_tree(updated_tree_oid)?;

        let message = format!(
            "Benchmark data for {}\n\nTarget-Commit: {}",
            commit_hash,
            commit_hash
        );

        let new_commit_oid = self.repo.commit(
            Some(&self.slug_ref),
            &sig,
            &sig,
            &message,
            &updated_tree,
            &[&branch_commit],
        )?;
    
        Ok(new_commit_oid.to_string())
    }

    pub fn add_note(&self, target_commit_hash: &str, note_message: &str) -> Result<(), SlugError> {
        let oid = git2::Oid::from_str(target_commit_hash).map_err(|e| SlugError::Git(e))?;
        let sig = git2::Signature::now("Slug", "slug@slug.internal").map_err(|e| SlugError::Git(e))?;
        
        self.repo.note(
            &sig,
            &sig,
            Some("refs/notes/slug"),
            oid,
            note_message,
            true // Overwrite if note already exists
        )?;

        Ok(())
    }

    pub fn read_file_slug(&self, file_path: &String) -> Result<Vec<u8>, SlugError> {
        let branch = self.repo.find_reference(&self.slug_ref)?;
        let branch_commit = branch.peel_to_commit()?;
        let tree = branch_commit.tree()?;

        // Get the current content of the file
        let entry = tree.get_name(file_path).ok_or_else(|| SlugError::Git(git2::Error::from_str("File not found in tree")))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
    }

    // Names of every test file stored in slug tree
    pub fn list_files(&self) -> Result<Vec<String>, SlugError> {
        // Missing ref = no history
        let branch = match self.repo.find_reference(&self.slug_ref) {
            Ok(branch) => branch,
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let tree = branch.peel_to_commit()?.tree()?;

        let mut names = Vec::new();
        for entry in tree.iter() {
            if let Some(name) = entry.name() {
                names.push(name.to_string());
            }
        }
        Ok(names)
    }

    pub fn file_exists(&self, file_path: &String) -> Result<bool, SlugError> {
        // Missing ref = no history
        let branch = match self.repo.find_reference(&self.slug_ref) {
            Ok(branch) => branch,
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let branch_commit = branch.peel_to_commit()?;
        let tree = branch_commit.tree()?;

        let path = Path::new(file_path);
        match tree.get_path(path) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

}

pub fn get_commit_hash() -> Result<String, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

// Clean every trace after Slug, shared history, local history, notes
// Deleting ref drops its commits from the graph and git garbage collects them
pub fn clean() -> Result<Vec<String>, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let mut removed = Vec::new();

    for refname in [SHARED_REF, LOCAL_REF, NOTES_REF] {
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

