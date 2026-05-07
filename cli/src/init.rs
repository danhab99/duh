use std::error::Error;

use lib::repo::Repo;
use lib::utils;
use vfs::PhysicalFS;

pub fn init() -> Result<(), Box<dyn Error>> {
    let _ = Repo::initialize_at(utils::get_cwd(), PhysicalFS::new("/"))?;
    println!("{}", crate::colors::green("Initialized new DUH directory"));
    Ok(())
}
