use git2;
use std::path::{Path, PathBuf};
use std::fs;
use crate::errors::SlugError;


pub struct SlugGit {
    pub repo: git2::Repository,
    pub slug_branch_ref: String,
}


impl SlugGit {
    pub fn new() -> Result<Self, SlugError> {
        let repo = git2::Repository::discover(".")?;

        let instance = Self {
            repo: repo,
            slug_branch_ref: "refs/heads/slug".to_string(),
        };
        instance.ensure_branch_exists("slug")?;
        
        Ok(instance)
    }


    pub fn get_path(&self) -> Result<PathBuf, SlugError> {
        Ok(self.repo.path().parent().expect("Parent path should always exist").to_path_buf())
    }


    pub fn get_commit_hash(&self) -> Result<String, SlugError> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        Ok(commit.id().to_string())
    }


    pub fn branch_exists(&self, branch_name: &str) -> Result<bool, SlugError> {
        match self.repo.find_branch(branch_name, git2::BranchType::Local) {
            Ok(_) => Ok(true),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }


    pub fn create_branch(&self, branch_name: &str) -> Result<(), SlugError> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;

        let first_commit = commit.parents().nth_back(0);

        let first_commit = match first_commit {
            Some(ancestor) => ancestor,
            None => commit,
        };
        
        self.repo.branch(branch_name, &first_commit, false)?;
        
        Ok(())
    }


    pub fn ensure_branch_exists(&self, branch_name: &str) -> Result<(), SlugError> {
        if !self.branch_exists(branch_name)? {
            self.create_branch(branch_name)?;
        }
        Ok(())
    }


    pub fn get_cur_branch(&self) -> Result<String, SlugError> {        
        let head = self.repo.head()?;
        head.shorthand()
            .map(String::from)
            .ok_or_else(|| SlugError::Git(git2::Error::from_str("HEAD is not pointing to a branch")))
    }


    pub fn edit_branch_slug(&self, test_name: &String, test_data: &String) -> Result<(), SlugError> {
        let branch_ref = format!("refs/heads/slug");
        let mut branch = self.repo.find_reference(&branch_ref)?;
        let branch_commit = branch.peel_to_commit()?;


        // Prepare a TreeBuilder and write test_data to the test_name file
        let tree = branch_commit.tree()?;
        let mut tree_builder = self.repo.treebuilder(Some(&tree))?;

        let mut current_content = String::new();
        if self.file_exists(test_name)? {
            // Get the current content of the file
            let entry = tree.get_name(test_name).ok_or_else(|| SlugError::Git(git2::Error::from_str("File not found in tree")))?;
            let blob = self.repo.find_blob(entry.id())?;
            current_content = String::from_utf8(blob.content().to_vec())
                .map_err(|e| SlugError::Parsing(e.to_string()))?;
        }

        // Append new data to the existing content
        current_content.push_str(test_data);

        let content_oid = self.repo.blob(current_content.as_bytes())?;
        tree_builder.insert(test_name, content_oid, 0o100644)?;
        let updated_tree_oid = tree_builder.write()?;

        // Create a new commit on this updated tree and use the parent commit
        let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
        let updated_tree = self.repo.find_tree(updated_tree_oid)?;

        self.repo.commit(
            Some(&branch_ref),
            &sig,
            &sig,
            &format!("Updating {} on slug", test_name),
            &updated_tree,
            &[&branch_commit],
        )?;
    
        Ok(())
    }


    pub fn read_file_slug(&self, file_path: &String) -> Result<Vec<u8>, SlugError> {
        let mut branch = self.repo.find_reference(&self.slug_branch_ref)?;
        let branch_commit = branch.peel_to_commit()?;
        let tree = branch_commit.tree()?;

        // Get the current content of the file
        let entry = tree.get_name(file_path).ok_or_else(|| SlugError::Git(git2::Error::from_str("File not found in tree")))?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
    }


    pub fn file_exists(&self, file_path: &String) -> Result<bool, SlugError> {    
        let branch_ref = format!("refs/heads/slug");
        let mut branch = self.repo.find_reference(&branch_ref)?;
        let branch_commit = branch.peel_to_commit()?;
        let tree = branch_commit.tree()?;

        let path = Path::new(file_path);
        match tree.get_path(path) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }


    fn add_all_in_dir(index: &mut git2::Index, dir: &Path) -> Result<(), SlugError> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::add_all_in_dir(index, &path)?;
            } else {
                index.add_path(&path)?;
            }
        }
        Ok(())
    }
    

    pub fn amend_slug(&self) -> Result<(), SlugError> {
        let head = self.repo.head()?;
        let commit = head.peel_to_commit()?;
        
        let mut index = self.repo.index()?;
        let slug_path = Path::new(".slug");
        Self::add_all_in_dir(&mut index, slug_path)?;
        index.write()?;
        
        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
    
        commit.amend(
            Some("HEAD"),
            None,
            None,
            None,
            None,
            Some(&tree)
        )?;
        
        Ok(())
    }
}

pub fn get_commit_hash() -> Result<String, SlugError> {
    let repo = git2::Repository::discover(".")?;
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    Ok(commit.id().to_string())
}

pub fn get_path() -> Result<PathBuf, SlugError> {
    let repo = git2::Repository::discover(".")?;
    Ok(repo.path().parent().expect("Parent path should always exist").to_path_buf())
}
