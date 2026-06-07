use crate::{
    hash::Hash,
    objects::{FileFragment, Object},
    space::Space,
    vlog,
};
use std::error::Error;
use vfs::FileSystem;

pub fn fetch_all_refs<L: FileSystem, R: FileSystem>(
    local: &mut Space<L>,
    remote: &mut Space<R>,
    remote_name: &str,
    prefix: &str,
) -> Result<(), Box<dyn Error>> {
    vlog!("remote::fetch_all_refs");

    let remote_refs = remote.list_refs("/")?;

    for r in remote_refs {
        local.set_ref(
            format!(
                "{}/{}/{}",
                prefix,
                remote_name,
                r.clone().to_string().as_str()
            )
            .as_str(),
            r,
        )?;
    }

    Ok(())
}

pub enum CopyCommitsProgress {
    Commit(Hash),
}

pub fn copy_commits<L: FileSystem, R: FileSystem, P>(
    src: &mut Space<L>,
    dest: &mut Space<R>,
    hash: Hash,
    progress: Option<P>,
) -> Result<(), Box<dyn Error>>
where
    P: Fn(CopyCommitsProgress),
{
    vlog!("remote::fetch_commit_from_remote {}", hash.to_hex());

    if let Some(_) = dest.get_object(hash)? {
        return Ok(());
    }

    if let Some(Object::Commit(commit)) = src.get_object(hash)? {
        copy_commits(src, dest, commit.parent, progress)?;
        dest.save_obj(Object::Commit(commit.clone()))?;

        for (_, hash) in commit.files.iter() {
            vlog!(
                "remote::fetch_commit_from_remote transferred object {}",
                hash.to_hex()
            );

            let obj = if let Some(Object::File(obj)) = src.get_object(*hash)? {
                dest.save_obj(Object::File(obj.clone()))?;
                obj
            } else {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "file",
                    "didn't receive a file",
                )));
            };

            for frag_hash in obj.fragments.iter() {
                if let Some(Object::FileDiffFragment(frag)) = dest.get_object(*frag_hash)? {
                    dest.save_obj(Object::FileDiffFragment(frag.clone()))?;

                    match frag {
                        FileFragment::ADDED { body, len } => {
                            if let Some(f) = dest.get_object(body)? {
                                dest.save_obj(f)?;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        if let Some(p) = progress {
            p(CopyCommitsProgress::Commit(hash));
        }
    } else {
        return Err(Box::new(crate::error::DuhError::invalid_object(
            "commit",
            "unknown object type",
        )));
    }

    Ok(())
}
