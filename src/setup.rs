use crate::git;

use std::fs;
use std::io::Write;
use std::error::Error;
use std::path::PathBuf;
use std::os::unix::fs::PermissionsExt;


const CUSTOM_MERGE_SCRIPT: &str = r#"#!/bin/bash
if [ "$#" -ne 4 ]; then
    echo "Bad number of arguments, expected 4"
    exit 1
fi
base_file=$1
current_file=$2
other_file=$3
conflict_line=$4
# get unique lines
tail -n +2 "$other_file" | grep -Fxv -f <(tail -n +2 "$current_file") >> "$current_file"
"#;

const HOOKS: &str = r#"[merge "slug_merger"]
    name = Merge slug .csv data
    driver = .slug/scripts/slug_merger
"#;

pub fn setup() -> Result<(), Box<dyn Error>> {
    let git_path = git::get_path()?;
    //println!("PATH = {}", git_path.to_str().unwrap_or("PATH is not valid UTF-8"));
    
    //setup_hooks(&git_path)?;
    setup_gitattributes(&git_path)?;
    create_slug_script(&git_path)?;

    Ok(())
}

fn setup_hooks(git_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let hooks_path = git_path.join(".git/hooks");

    if !hooks_path.exists() {
        fs::File::create(&hooks_path)?;
        fs::write(&hooks_path, HOOKS)?;
    }

    // Append slug merge hook
    let mut hooks_content = fs::read_to_string(&hooks_path)?;
    if !hooks_content.contains(HOOKS) {
        hooks_content.push_str(HOOKS);
        fs::write(&hooks_path, hooks_content)?;
    }

    Ok(())
}

fn setup_gitattributes(git_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let gitattributes_path = git_path.join(".gitattributes");

    if !gitattributes_path.exists() {
        fs::File::create(&gitattributes_path)?;
        fs::write(&gitattributes_path, "plaintext\n")?;
    }

    // Append .slug merge rules
    let merge_rule = "\n.slug/*.csv merge=slug_merger\n";
    let mut attributes_content = fs::read_to_string(&gitattributes_path)?;
    if !attributes_content.contains(merge_rule) {
        attributes_content.push_str(merge_rule);
        fs::write(&gitattributes_path, attributes_content)?;
    }

    Ok(())
}

fn create_slug_script(git_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let dir_path = git_path.join(".slug/scripts");
    let file_path = dir_path.join("custom-merge.sh");

    if !dir_path.exists() {
        fs::create_dir_all(&dir_path)?;
    }

    let mut file = fs::File::create(&file_path)?;
    file.write_all(CUSTOM_MERGE_SCRIPT.as_bytes())?;

    // Make the file executable
    let metadata = fs::metadata(&file_path)?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(0o755); // rwxr-xr-x
    fs::set_permissions(&file_path, permissions)?;

    Ok(())
}