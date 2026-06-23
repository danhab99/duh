use std::error::Error;

use lib::space::Space;
use opendal::services::Fs;

pub fn init() -> Result<(), Box<dyn Error>> {
    let op = Fs::default().root(std::env::current_dir()?.to_str().unwrap());

    let afs = opendal::Operator::new(op)?.finish();
    let fs = opendal::blocking::Operator::new(afs)?;

    let _ = Space::initialize_at(fs)?;
    println!("{}", crate::colors::green("Initialized new DUH directory"));
    Ok(())
}
