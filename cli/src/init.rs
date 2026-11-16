use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;
use opendal::services::Fs;

#[derive(Args)]
#[command(about = "Initialize a new space")]
pub struct InitCommand {
    #[arg(long = "bare", help = "Initialize a bare space (no working tree)")]
    pub bare: bool,
}

pub fn init(cmd: &InitCommand) -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let duh_dir = cwd.join(lib::utils::SPACE_METADATA_DIR_NAME);
    let op = Fs::default().root(duh_dir.to_str().unwrap());

    let fs = opendal::Operator::new(op)?.finish().blocking();

    let worktree = if cmd.bare { None } else { Some(cwd) };
    let _ = Space::initialize_at(fs, worktree)?;

    if cmd.bare {
        println!("{}", crate::colors::green("Initialized new bare DUH directory"));
    } else {
        println!("{}", crate::colors::green("Initialized new DUH directory"));
    }
    Ok(())
}
