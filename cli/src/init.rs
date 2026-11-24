use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Initialize a new space")]
pub struct InitCommand {
    #[arg(long = "bare", help = "Initialize a bare space (no working tree)")]
    pub bare: bool,
}

pub fn init(cmd: &InitCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let duh_dir = cwd.join(lib::utils::SPACE_METADATA_DIR_NAME);
    let worktree = if cmd.bare { None } else { Some(cwd) };
    let _ = Space::init(lib::store::local(&duh_dir)?, worktree)?;

    if cmd.bare {
        println!("{}", crate::colors::green("Initialized new bare DUH directory"));
    } else {
        println!("{}", crate::colors::green("Initialized new DUH directory"));
    }
    Ok(())
}
