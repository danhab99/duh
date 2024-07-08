use crate::repo::Repo;

pub fn status(wd: Option<String>) {
    let repo = Repo::at_root_path(wd);

    let files = std::fs::read_dir(repo.get_path_in_cwd(""))
        .unwrap()
        .map(|x| x.unwrap())
        .collect::<Vec<_>>();

    println!("STATUS {:?}", files);
}
