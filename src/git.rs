use std::error::Error;
use git2;
use std::path::{Path, PathBuf};
use std::fs;


pub struct SlugGit {
    pub repo: git2::Repository,
    pub slug_branch_ref: String,
}


impl SlugGit {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git repository: {}", e))?;

        let instance = Self {
            repo: repo,
            slug_branch_ref: "refs/heads/slug".to_string(),
        };
        instance.ensure_branch_exists("slug")?;
        
        Ok(instance)
    }


    pub fn get_path(&self) -> Result<PathBuf, Box<dyn Error>> {
        Ok(self.repo.path().parent().expect("Parent path should always exist").to_path_buf())
    }


    pub fn get_commit_hash(&self) -> Result<String, Box<dyn Error>> {
        let head = self.repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
        let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
        Ok(commit.id().to_string())
    }


    pub fn branch_exists(&self, branch_name: &str) -> Result<bool, Box<dyn Error>> {
        match self.repo.find_branch(branch_name, git2::BranchType::Local) {
            Ok(_) => Ok(true),
            Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
            Err(e) => Err(Box::new(e)),
        }
    }


    pub fn create_branch(&self, branch_name: &str) -> Result<(), Box<dyn Error>> {
        let head = self.repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
        let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;

        let first_commit = commit.parents().nth_back(0);

        let first_commit = match first_commit {
            Some(ancestor) => ancestor,
            None => commit,
        };
        
        self.repo.branch(branch_name, &first_commit, false).map_err(|e| format!("Failed to create branch: {}", e))?;
        
        Ok(())
    }


    pub fn ensure_branch_exists(&self, branch_name: &str) -> Result<(), Box<dyn Error>> {
        if !self.branch_exists(branch_name)? {
            self.create_branch(branch_name)?;
        }
        Ok(())
    }


    pub fn get_cur_branch(&self) -> Result<String, Box<dyn Error>> {        
        let head = self.repo.head()?;
        head.shorthand().map(String::from).ok_or_else(|| "HEAD is not pointing to a branch".into())
    }


    pub fn edit_branch_slug(&self, test_name: &String, test_data: &String) -> Result<(), Box<dyn std::error::Error>> {
        let branch_ref = format!("refs/heads/slug");
        let mut branch = self.repo.find_reference(&branch_ref)?;
        let branch_commit = branch.peel_to_commit()?;


        // Prepare a TreeBuilder and write test_data to the test_name file
        let tree = branch_commit.tree()?;
        let mut tree_builder = self.repo.treebuilder(Some(&tree))?;

        let mut current_content = String::new();
        if self.file_exists(test_name)? {
            // Get the current content of the file
            let entry = tree.get_name(test_name).ok_or("File not found in tree")?;
            let blob = self.repo.find_blob(entry.id())?;
            current_content = String::from_utf8(blob.content().to_vec())?;
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


    pub fn read_file_slug(&self, file_path: &String) -> Result<Vec<u8>, Box<dyn Error>> {
        let mut branch = self.repo.find_reference(&self.slug_branch_ref)?;
        let branch_commit = branch.peel_to_commit()?;
        let tree = branch_commit.tree()?;

        // Get the current content of the file
        let entry = tree.get_name(file_path).ok_or("File not found in tree")?;
        let blob = self.repo.find_blob(entry.id())?;
        Ok(blob.content().to_vec())
        //let current_content = String::from_utf8(blob.content().to_vec())?;

        //Ok(current_content)
    }


    pub fn file_exists(&self, file_path: &String) -> Result<bool, Box<dyn Error>> {    
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


    fn add_all_in_dir(index: &mut git2::Index, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
    

    pub fn amend_slug(&self) -> Result<(), Box<dyn Error>> {
        let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
        let head = self.repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
        let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
        
        let mut index = self.repo.index().map_err(|e| format!("Failed to get index: {}", e))?;
        let slug_path = Path::new(".slug");
        Self::add_all_in_dir(&mut index, slug_path)?;
        index.write().map_err(|e| format!("Failed to write index: {}", e))?;
        
        let tree_id = index.write_tree().map_err(|e| format!("Failed to write tree: {}", e))?;
        let tree = self.repo.find_tree(tree_id).map_err(|e| format!("Failed to find tree: {}", e))?;
    
        commit.amend(
            Some("HEAD"),
            None,
            None,
            None,
            None,
            Some(&tree)
        ).map_err(|e| format!("Failed to amend commit: {}", e))?;
        
        Ok(())
    }
}

pub fn get_commit_hash() -> Result<String, Box<dyn Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    Ok(commit.id().to_string())
}

pub fn get_path() -> Result<PathBuf, Box<dyn Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    Ok(repo.path().parent().expect("Parent path should always exist").to_path_buf())
}
