use crate::{
    hash::Hash,
    objects::{FileFragment, Object},
    repo::Repo,
    vlog,
};
use std::error::Error;
use vfs::FileSystem;

pub fn fetch_all_refs<L: FileSystem, R: FileSystem>(
    local: &mut Repo<L>,
    remote: &mut Repo<R>,
    remote_name: &str,
) -> Result<(), Box<dyn Error>> {
    vlog!("remote::fetch_all_refs");

    let remote_refs = remote.list_refs("/")?;

    for r in remote_refs {
        let head_hash = remote.resolve_ref_name(r.clone())?;

        local.set_ref(
            format!("{}/{}", remote_name, r.clone().to_string().as_str()).as_str(),
            r,
        )?;

        fetch_commit_from_remote(local, remote, head_hash)?;
    }

    Ok(())
}

pub fn fetch_commit_from_remote<L: FileSystem, R: FileSystem>(
    local: &mut Repo<L>,
    remote: &mut Repo<R>,
    hash: Hash,
) -> Result<(), Box<dyn Error>> {
    vlog!("remote::fetch_commit_from_remote {}", hash.to_hex());

    if let Some(_) = local.get_object(hash)? {
        return Ok(());
    }

    let commit_object = remote.get_object(hash)?;

    if let Some(Object::Commit(commit)) = commit_object {
        local.save_obj(Object::Commit(commit.clone()))?;

        for (_, hash) in commit.files.iter() {
            vlog!(
                "remote::fetch_commit_from_remote transferred object {}",
                hash.to_hex()
            );

            if let Some(Object::FileVersion(obj)) = remote.get_object(*hash)? {
                local.save_obj(Object::FileVersion(obj.clone()))?;

                for frag in obj.fragments.iter() {
                    if let FileFragment::ADDED { body, .. } = frag {
                        if let Some(f) = remote.get_object(*body)? {
                            local.save_obj(f)?;
                        }
                    }
                }
            }
        }

        fetch_commit_from_remote(local, remote, commit.parent)?;
    } else {
        panic!("not a commit");
    }

    Ok(())
}

pub fn push_branch_to_remote<L: FileSystem, R: FileSystem, F: Fn(Hash)>(
    local: &mut Repo<L>,
    remote: &mut Repo<R>,
    hash: Hash,
    progress: F,
) -> Result<(), Box<dyn Error>> {
    let x = local.get_object(hash)?.unwrap();

    if let Object::Commit(c) = x {
        push_branch_to_remote(local, remote, c.parent, progress)?;

        let h = remote.save_obj(Object::Commit(c.clone()))?;
        progress(h);

        for (_, h) in c.files.iter() {
            let o = local.get_object(*h)?.unwrap();
            remote.save_obj(o.clone())?;

            if let Object::File(f) = o {
                for frag_hash in f.fragments {
                    if let Some(frag) = local.get_object(frag_hash)? {
                        remote.save_obj(Object::FileDiffFragment(frag))?;
                    }
                }
            }
        }
    } else {
        panic!("not a commit");
    }

    Ok(())
}
