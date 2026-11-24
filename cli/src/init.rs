use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Start a new space in this folder")]
pub struct InitCommand {
    #[arg(long = "bare", help = "Only history, no working files (for sharing)")]
    pub bare: bool,
}

pub fn init(cmd: &InitCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let duh_dir = cwd.join(lib::utils::SPACE_METADATA_DIR_NAME);
    let worktree = if cmd.bare { None } else { Some(cwd) };
    let _ = Space::init(lib::store::local(&duh_dir)?, worktree)?;

    if cmd.bare {
        println!("{}", crate::colors::green("Started a bare space here (history only, for sharing)"));
    } else {
        println!("{}", crate::colors::green("Started a new space here. `duh pick <file>` to begin tracking files."));
    }
    Ok(())
}
