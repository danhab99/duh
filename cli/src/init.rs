use std::error::Error;

use lib::repo::Repo;
use lib::utils;

pub fn init() -> Result<(), Box<dyn Error>> {
    let _ = Repo::initialize_at(utils::get_cwd())?;
    println!("{}", crate::colors::green("Initialized new DUH directory"));
    Ok(())
}
