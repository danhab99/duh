use std::error::Error;

use clap::{clap_derive::Args, Subcommand};
use lib::space::Space;

#[derive(Args)]
#[command(about = "Get or set spacesitory configuration values")]
pub struct ConfigCommand {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Get a config value (e.g. user.name, chunk_size)
    Get {
        /// Dot-separated key (e.g. `user.name`)
        key: String,
    },
    /// Set a config value (e.g. user.name, chunk_size)
    Set {
        /// Dot-separated key (e.g. `user.name`)
        key: String,
        /// Value to assign
        value: String,
    },
}

pub fn config<F: vfs::FileSystem>(space: &Space<F>, cmd: &ConfigCommand) -> Result<(), Box<dyn Error>> {
    match &cmd.action {
        ConfigAction::Get { key } => {
            let val = space.get_config_value(key)?;
            println!("{}", val);
        }
        ConfigAction::Set { key, value } => {
            space.set_config_value(key, value)?;
            println!(
                "{} {} = {}",
                crate::colors::green("set"),
                crate::colors::cyan(key),
                value
            );
        }
    }
    Ok(())
}
