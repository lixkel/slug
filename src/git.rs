use std::error::Error;
use git2::Repository;
use std::path::Path;
use std::fs;


// TODO: create custom errors finally
fn add_all_in_dir(index: &mut git2::Index, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_all_in_dir(index, &path)?;
        } else {
            index.add_path(&path)?;
        }
    }
    Ok(())
}

pub fn get_commit_hash() -> Result<String, Box<dyn Error>> {
    let repo = Repository::discover(".").map_err(|e| format!("Failed to discover repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    Ok(commit.id().to_string())
}

pub fn amend_slug() -> Result<(), Box<dyn Error>> {
    let repo = Repository::discover(".").map_err(|e| format!("Failed to discover repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    
    let mut index = repo.index().map_err(|e| format!("Failed to get index: {}", e))?;
    let slug_path = Path::new(".slug");
    add_all_in_dir(&mut index, slug_path)?;
    index.write().map_err(|e| format!("Failed to write index: {}", e))?;
    
    let tree_id = index.write_tree().map_err(|e| format!("Failed to write tree: {}", e))?;
    let tree = repo.find_tree(tree_id).map_err(|e| format!("Failed to find tree: {}", e))?;

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

pub fn branch_exists(repo: &Repository, branch_name: &str) -> Result<bool, Box<dyn Error>> {
    match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(_) => Ok(true),
        Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => Err(Box::new(e)),
    }
}

pub fn create_branch(repo: &Repository, branch_name: &str) -> Result<(), Box<dyn Error>> {
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    
    repo.branch(branch_name, &commit, false).map_err(|e| format!("Failed to create branch: {}", e))?;
    
    Ok(())
}

pub fn ensure_branch_exists(repo: &Repository, branch_name: &str) -> Result<(), Box<dyn Error>> {
    if !branch_exists(repo, branch_name)? {
        create_branch(repo, branch_name)?;
    }
    Ok(())
}

pub fn checkout_branch(branch_name: &str) -> Result<(), Box<dyn StdError>> {
    let repo = Repository::discover(".").map_err(|e| format!("Failed to discover repository: {}", e))?;
    ensure_branch_exists(&repo, branch_name)?;
    
    repo.set_head(&format!("refs/heads/{}", branch_name)).map_err(|e| format!("Failed to set HEAD: {}", e))?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force())).map_err(|e| format!("Failed to checkout branch: {}", e))?;
    
    Ok(())
}

pub fn get_cur_branch() -> Result<String, Box<dyn StdError>> {
    let repo = Repository::discover(".")?;
    
    let head = repo.head()?;
    head.shorthand().ok_or_else(|| "HEAD is not pointing to a branch".into())
}