use lib::utils;

pub fn status(wd: Option<String>) {
    let cwd = utils::get_cwd();
    let path = wd.to_owned().unwrap_or(cwd.clone());

    if !path.starts_with(&cwd) {
        panic!("no repo found");
    }

    let root = utils::find_repo_root(Some(path.clone())).unwrap();

    let files = std::fs::read_dir(root)
        .unwrap()
        .map(|x| x.unwrap())
        .collect::<Vec<_>>();

    println!("STATUS {} {:?}", path, files);
}
