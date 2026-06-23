use std::error::Error;

use lib::space::Space;
use opendal::services::Fs;

pub fn init() -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir()?;
    let op = Fs::default().root(cwd.to_str().unwrap());

    let afs = opendal::Operator::new(op)?.finish();
    let fs = opendal::blocking::Operator::new(afs)?;

    // For init, worktree is the current directory (where .duh will be created)
    let _ = Space::initialize_at(fs, Some(cwd))?;
    println!("{}", crate::colors::green("Initialized new DUH directory"));
    Ok(())
}
