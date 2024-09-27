use lib::utils;
use lib::repo::Repo;

pub fn init() {
    let _ = Repo::initalize_at(utils::get_cwd());
    println!("Initialized new DUH directory");
}
