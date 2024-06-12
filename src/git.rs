use std::error::Error;
use git2::Repository;

pub fn get_commit_hash() -> Result<String, Box<dyn Error>> {
    let repo = Repository::discover(".").map_err(|e| format!("Failed to discover repository: {}", e))?;
    let head = repo.head().map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let commit = head.peel_to_commit().map_err(|e| format!("Failed to peel to commit: {}", e))?;
    Ok(commit.id().to_string())
}