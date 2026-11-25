//! Saving a new version: `duh save`.

use super::look::look;
use super::pick::{pick_deletion, pick_file, PickEvent, Picked};
use super::{fmt_bytes, OpResult};
use crate::file::FileOps;
use crate::hash::Hash;
use crate::space::Space;

/// A save just made.
#[derive(Debug, Clone)]
pub struct Saved {
    pub version: Hash,
    /// The draft it went on, or `None` when looking at a version directly.
    pub draft: Option<String>,
    pub message: String,
}

/// Pick every changed or missing tracked file (`save -a`), reporting each.
pub fn pick_tracked<F: FnMut(PickEvent)>(space: &mut Space, mut on: F) -> OpResult<Vec<Picked>> {
    let report = look(space)?;
    let mut out = Vec::new();
    for p in &report.changed {
        out.push(pick_file(space, p, &mut on)?);
    }
    for p in &report.missing {
        let picked = pick_deletion(space, p)?;
        on(PickEvent::Done(&picked));
        out.push(picked);
    }
    Ok(out)
}

/// Save what's picked as a new version on the open draft.
pub fn save(space: &mut Space, message: &str, allow_empty: bool) -> OpResult<Saved> {
    if space.index.is_empty() && !allow_empty {
        return Err("nothing to save: pick some changes first (`duh pick <file>`), or use --allow-empty".into());
    }
    if message.trim().is_empty() {
        return Err("a save needs a message saying what changed".into());
    }
    let version = FileOps::from_space(space).commit(message.to_string())?;
    Ok(Saved { version, draft: space.get_head_branch_name().ok(), message: message.to_string() })
}

/// A message describing what's picked, e.g. "Update 2 files" with a line
/// per file. `display` turns a repo path into how it should be written
/// (the CLI shows paths relative to where you are).
pub fn suggest_message(space: &mut Space, display: impl Fn(&str) -> String) -> OpResult<String> {
    let mut summaries = FileOps::from_space(space).staged_summary()?;
    if summaries.is_empty() {
        return Ok("Nothing changed".to_string());
    }
    summaries.sort_by(|a, b| a.path.cmp(&b.path));

    let noun = if summaries.len() == 1 { "file" } else { "files" };
    let mut lines = vec![format!("Update {} {}", summaries.len(), noun), String::new()];
    for s in &summaries {
        let name = display(&s.path);
        if s.removed {
            lines.push(format!("  {}  (deleted)", name));
        } else if s.unchanged_bytes == 0 && s.deleted_bytes == 0 {
            lines.push(format!("  {}  +{}  (new)", name, fmt_bytes(s.added_bytes as u64)));
        } else {
            let mut parts = vec![format!("+{}", fmt_bytes(s.added_bytes as u64))];
            if s.deleted_bytes > 0 {
                parts.push(format!("-{}", fmt_bytes(s.deleted_bytes as u64)));
            }
            parts.push(format!("~{} unchanged", fmt_bytes(s.unchanged_bytes as u64)));
            lines.push(format!("  {}  {}", name, parts.join("  ")));
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::super::testing;
    use super::*;

    #[test]
    fn save_needs_picks_and_a_message() {
        let (dir, mut space) = testing::space();
        assert!(save(&mut space, "x", false).unwrap_err().to_string().contains("nothing to save"));
        testing::write(&dir, "a.bin", &testing::noise(1, 10_000));
        pick_file(&mut space, "a.bin", |_| {}).unwrap();
        assert!(save(&mut space, "  ", false).is_err());
        assert_eq!(suggest_message(&mut space, |p| p.to_string()).unwrap().lines().next(), Some("Update 1 file"));
        let saved = save(&mut space, "first", false).unwrap();
        assert_eq!(saved.draft.as_deref(), Some("main"));
        assert!(space.index.is_empty());
        assert!(crate::ops::open(dir.path()).unwrap().index.is_empty());
    }
}
