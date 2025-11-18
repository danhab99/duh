use std::error::Error;
use std::path::PathBuf;

use clap::clap_derive::Args;
use lib::repo::Repo;

#[derive(Args)]
pub struct SnapshotCommand {
    /// Path to the file to snapshot
    pub file_path: String,
    
    /// Commit message
    #[arg(short = 'm', long = "message", default_value = "Snapshot commit")]
    pub message: String,
}

pub fn snapshot(repo: Repo, cmd: &SnapshotCommand) -> Result<(), Box<dyn Error>> {
    // Convert the file path string to a PathBuf
    let path = repo.get_path_in_cwd(&cmd.file_path);
    
    // Check if the file exists
    if !path.exists() {
        eprintln!("Error: File '{}' does not exist", path.as_os_str().to_str().unwrap());
        std::process::exit(1);
    }
    
    if !path.is_file() {
        eprintln!("Error: '{}' is not a file", path.as_os_str().to_str().unwrap());
        std::process::exit(1);
    }
    
    println!("Creating snapshot for: {}", cmd.file_path);
    
    // Commit the file using our crown jewel function!
    match repo.commit_file(&path, cmd.message.clone()) {
        Ok(commit_hash) => {
            println!("✓ Snapshot created successfully!");
            println!("Commit hash: {}", commit_hash.to_string());
            
            // Optionally show what was tracked
            let file_size = std::fs::metadata(&path)?.len();
            println!("File size: {} bytes", file_size);
            
            Ok(())
        }
        Err(e) => {
            eprintln!("Error creating snapshot: {}", e);
            Err(e)
        }
    }
}
