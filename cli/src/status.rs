use std::error::Error;

use clap::clap_derive::Args;
use lib::ops::look::{look, PickedChange};
use lib::space::Space;

#[derive(Args)]
#[command(about = "See what's changed, picked and new")]
pub struct StatusCommand {}

pub fn status(space: &mut Space, _cmd: &StatusCommand) -> Result<(), Box<dyn Error>> {
    let report = look(space)?;
    let cwd = std::env::current_dir()?;

    let draft = match &report.draft {
        Some(name) => name.clone(),
        None => format!("none (looking at version {})", &report.head.to_string()[..12]),
    };
    println!("On draft {}", crate::colors::green(&draft));

    let picked: Vec<String> = report
        .picked
        .iter()
        .map(|(p, c)| {
            let p = space.display_path(&cwd, p);
            match c {
                PickedChange::New => format!("{} (new)", p),
                PickedChange::Modified => p,
                PickedChange::Deleted => format!("{} (deleted)", p),
            }
        })
        .collect();
    let rel = |paths: &Vec<String>| -> Vec<String> { paths.iter().map(|p| space.display_path(&cwd, p)).collect() };
    let sections: [(&str, Vec<String>, fn(&str) -> String); 5] = [
        ("Picked for the next save:", picked, crate::colors::green),
        ("Changed, not picked:", rel(&report.changed), crate::colors::yellow),
        ("Missing:", rel(&report.missing), crate::colors::red),
        ("New, not tracked yet:", rel(&report.new), crate::colors::red),
        ("Saved, unchanged:", rel(&report.unchanged), crate::colors::dim),
    ];
    let mut any = false;
    for (title, paths, color) in sections {
        if paths.is_empty() {
            continue;
        }
        any = true;
        println!();
        println!("{}", color(title));
        for p in paths {
            println!("  {}", color(&p));
        }
    }
    if !any {
        println!();
        println!("{}", crate::colors::dim("Nothing here yet. Use `duh pick <file>` to start tracking files."));
    }
    Ok(())
}
