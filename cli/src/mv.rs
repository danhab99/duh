use std::error::Error;
use std::path::Path;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Move or rename a tracked file")]
pub struct MvCommand {
    /// Tracked file to move
    pub source: String,

    /// New path (or an existing directory to move it into)
    pub destination: String,
}

pub fn mv(space: &mut Space, cmd: &MvCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let src = space.repo_path_from(&cwd, &cmd.source)?;

    // Moving into an existing directory keeps the file name.
    let mut dest_arg = Path::new(&cmd.destination).to_path_buf();
    if dest_arg.is_dir() {
        if let Some(name) = Path::new(&src).file_name() {
            dest_arg.push(name);
        }
    }
    let dest = space.repo_path_from(&cwd, &dest_arg.to_string_lossy())?;

    lib::ops::files::rename(space, &src, &dest)?;
    println!("Renamed {} -> {}", src, dest);
    Ok(())
}
