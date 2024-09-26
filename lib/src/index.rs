
#[derive(Debug, Serialize, Deserialize)]
pub struct StagedFile {
    content_hash: Hash,
    filestruct_hash: Hash,
    file_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Index {
    staged_files: Vec<StagedFile>,
}

