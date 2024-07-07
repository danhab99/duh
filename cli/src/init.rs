use lib::utils;
use crate::repo::Repo;

pub fn init() {
    let _ = Repo::initalize_at(Some(utils::get_cwd()));
    println!("Initialized new DUH directory");
}
