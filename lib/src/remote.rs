use crate::{
    hash::Hash,
    objects::{FileFragment, Object},
    space::Space,
    vlog,
};
use std::error::Error;

fn copy_file_version(
    src: &mut Space,
    dest: &mut Space,
    file_hash: Hash,
) -> Result<(), Box<dyn Error>> {
    let file_obj = match src.get_object(file_hash)? {
        Some(Object::File(obj)) => obj,
        _ => {
            return Err(Box::new(crate::error::DuhError::invalid_object(
                "file",
                "didn't receive a file",
            )))
        }
    };

    dest.save_obj(Object::File(file_obj.clone()))?;

    for frag_hash in file_obj.fragments.iter() {
        let frag = match src.get_object(*frag_hash)? {
            Some(Object::FileDiffFragment(frag)) => frag,
            _ => {
                return Err(Box::new(crate::error::DuhError::invalid_object(
                    "file diff fragment",
                    "didn't receive a file diff fragment",
                )))
            }
        };

        dest.save_obj(Object::FileDiffFragment(frag.clone()))?;

        if let FileFragment::ADDED { body, len: _ } = frag {
            if let Some(obj) = src.get_object(body)? {
                dest.save_obj(obj)?;
            }
        }
    }

    Ok(())
}

fn copy_tree_objects(
    src: &mut Space,
    dest: &mut Space,
    tree_hash: Hash,
) -> Result<(), Box<dyn Error>> {
    let tree = match src.get_object(tree_hash)? {
        Some(Object::Tree(t)) => t,
        _ => {
            return Err(Box::new(crate::error::DuhError::invalid_object(
                "tree",
                "didn't receive a tree",
            )))
        }
    };

    dest.save_obj(Object::Tree(tree.clone()))?;

    for entry in tree.entries {
        if entry.mode == 0o40000 {
            copy_tree_objects(src, dest, entry.hash)?;
        } else {
            copy_file_version(src, dest, entry.hash)?;
        }
    }

    Ok(())
}

pub enum CopyCommitsProgress {
    Commit(Hash),
}

pub fn copy_commits<P>(
    src: &mut Space,
    dest: &mut Space,
    hash: Hash,
    progress: Option<P>,
) -> Result<(), Box<dyn Error>>
where
    P: Fn(CopyCommitsProgress) + std::marker::Copy,
{
    vlog!("remote::fetch_commit_from_remote {}", hash.to_hex());

    if let Some(_) = dest.get_object(hash)? {
        return Ok(());
    }

    if let Some(Object::Commit(commit)) = src.get_object(hash)? {
        if !commit.parent.is_zero() {
            copy_commits(src, dest, commit.parent, progress)?;
        }

        dest.save_obj(Object::Commit(commit.clone()))?;

        // Preserve commit reachability by copying the full tree graph and
        // all file/fragments referenced by that tree.
        copy_tree_objects(src, dest, commit.tree)?;

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
