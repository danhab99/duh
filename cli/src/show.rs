use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::ObjectReference;
use lib::ops::history;
use lib::space::Space;

/// Inspect a version
#[derive(Args)]
#[command(about = "Inspect one version: who saved it, when, and its files")]
pub struct ShowCommand {
    #[arg(value_name = "VERSION", help = "Version to inspect: a label, short hash, HEAD~1, … (default: the last save)")]
    pub commit: Option<ObjectReference>,
}

pub fn show(space: &mut Space, cmd: &ShowCommand) -> Result<(), Box<dyn Error>> {
    let version = history::resolve(space, &cmd.commit.clone().unwrap_or(ObjectReference::Ref("HEAD".into())))?;
    if version.is_zero() {
        println!("No saves yet");
        return Ok(());
    }
    let d = history::inspect(space, version)?;
    println!("version {}", crate::colors::cyan(&version.to_string()));
    if !d.info.previous.is_zero() {
        println!("previous: {}", crate::colors::dim(&d.info.previous.to_string()));
    }
    println!("saved by: {} <{}> {}", d.info.author_name, d.info.author_email, crate::when::utc(d.info.time));
    println!("\n    {}\n", d.info.message);
    println!("{}", crate::colors::bold("files:"));
    for f in &d.files {
        println!("  {} -> {}", crate::colors::cyan(&f.path), crate::colors::green(&f.version.to_string()));
    }
    Ok(())
}
