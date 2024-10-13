use std::error::Error;
use git2;
use std::path::{Path, PathBuf};
use std::fs;


pub fn get_path() -> Result<PathBuf, Box<dyn Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    Ok(repo.path().parent().expect("Parent path should always exist").to_path_buf())
}

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
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    Ok(commit.id().to_string())
}

pub fn amend_slug() -> Result<(), Box<dyn Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
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

pub fn branch_exists(repo: &git2::Repository, branch_name: &str) -> Result<bool, Box<dyn Error>> {
    println!("branch_name = {:?}", branch_name);
    match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(_) => Ok(true),
        Err(ref e) if e.code() == git2::ErrorCode::NotFound => Ok(false),
        Err(e) => Err(Box::new(e)),
    }
}

// TODO: make this a clear branch creation function
pub fn create_branch(repo: &git2::Repository, branch_name: &str) -> Result<(), Box<dyn Error>> {
    println!("branch_name = {:?}", branch_name);
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    
    repo.branch(branch_name, &commit, false).map_err(|e| format!("Failed to create branch: {}", e))?;
    
    Ok(())
}

pub fn ensure_branch_exists(repo: &git2::Repository, branch_name: &str) -> Result<(), Box<dyn Error>> {
    println!("branch_name = {:?}", branch_name);
    if !branch_exists(repo, branch_name)? {
        create_branch(repo, branch_name)?;
    }
    Ok(())
}

pub fn checkout_branch(branch_name: &str) -> Result<(), Box<dyn Error>> {
    println!("branch_name = {:?}", branch_name);
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    ensure_branch_exists(&repo, branch_name)?;
    
    repo.set_head(&format!("refs/heads/{}", branch_name)).map_err(|e| format!("Failed to set HEAD: {}", e))?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force())).map_err(|e| format!("Failed to checkout branch: {}", e))?;
    
    Ok(())
}

pub fn get_cur_branch() -> Result<String, Box<dyn Error>> {
    let repo = git2::Repository::discover(".")?;
    
    let head = repo.head()?;
    head.shorthand().map(String::from).ok_or_else(|| "HEAD is not pointing to a branch".into())
}

// Commit all changes and assume you are on head
pub fn commit_data(commit_msg: &str) -> Result<(), Box<dyn Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let parent_commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;

    // TODO: unite with ammend
    let mut index = repo.index().map_err(|e| format!("Failed to get index: {}", e))?;
    let slug_path = Path::new(".slug");
    add_all_in_dir(&mut index, slug_path)?;
    index.write().map_err(|e| format!("Failed to write index: {}", e))?;
    
    let tree_id = index.write_tree().map_err(|e| format!("Failed to write tree: {}", e))?;
    let tree = repo.find_tree(tree_id).map_err(|e| format!("Failed to find tree: {}", e))?;
    
    let signature = git2::Signature::now("Slug", "slug@slug.internal")?;
    
    repo.commit(
        Some("HEAD"),       // reference to update
        &signature,         // author of commit
        &signature,         // committer
        commit_msg,         // commit message
        &tree,              // tree object to commit
        &[&parent_commit],  // parent commit
    ).map_err(|e| format!("Failed to commit changes: {}", e))?;

    Ok(())
}


pub fn edit_branch_slug(test_name: &str, test_data: &str) -> Result<(), Box<dyn std::error::Error>> {
    let repo = git2::Repository::discover(".").map_err(|e| format!("Failed to discover git2::Repository: {}", e))?;
    ensure_branch_exists(&repo, "slug")?;

    let branch_ref = format!("refs/heads/slug");
    let mut branch = repo.find_reference(&branch_ref)?;
    let branch_commit = branch.peel_to_commit()?;


    // Prepare a TreeBuilder and write test_data to the test_name file
    let tree = branch_commit.tree()?;
    let mut tree_builder = repo.treebuilder(Some(&tree))?;
    let content_oid = repo.blob(test_data.as_bytes())?;
    tree_builder.insert(test_name, content_oid, 0o100644)?;
    let updated_tree_oid = tree_builder.write()?;

    // Create a new commit on this updated tree and use the parent commit
    let sig = git2::Signature::now("Slug", "slug@slug.internal")?;
    let updated_tree = repo.find_tree(updated_tree_oid)?;

    repo.commit(
        Some(&branch_ref),
        &sig,
        &sig,
        &format!("Updating {} on slug", test_name),
        &updated_tree,
        &[&branch_commit],
    )?;

    Ok(())
}