use std::error::Error;

use clap::clap_derive::Args;
use lib::space::Space;

#[derive(Args)]
#[command(about = "Open another draft, swapping your files to match it")]
pub struct SwitchCommand {
    #[arg(help = "Draft to open")]
    pub name: String,

    /// Start a new draft from where you are, then open it
    #[arg(short = 'n', long = "new", alias = "create", short_alias = 'c')]
    pub create: bool,
}

pub fn switch(space: &mut Space, cmd: &SwitchCommand) -> Result<(), Box<dyn Error>> {
    lib::ops::refs::open_draft(space, &cmd.name, cmd.create)?;
    println!("{} {}", crate::colors::cyan("Opened draft"), cmd.name);
    Ok(())
}
