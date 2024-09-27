
use lib::repo::Repo;

pub fn track(repo: Repo, names: &Vec<String>) {
    for n in names {
        repo.stage_file(n)?;
    }
}
