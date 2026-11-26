use std::error::Error;

use clap::clap_derive::Args;
use lib::check::CheckStep;
use lib::space::Space;

use crate::colors;

/// Check that every save is intact
#[derive(Args)]
#[command(about = "Check that every save is intact and reads back exactly")]
pub struct CheckCommand {
    /// A place to check instead of this space, e.g. a backup on S3
    pub place: Option<String>,
}

pub fn check(space: &mut Space, cmd: &CheckCommand) -> Result<(), Box<dyn Error>> {
    let mut target = match &cmd.place {
        Some(name) => {
            println!("Checking the place '{}'", name);
            space.get_remote_by_name(name)?
        }
        None => space.clone(),
    };

    let report = target.check(|step| {
        let what = match step {
            CheckStep::Objects => "stored pieces",
            CheckStep::Saves => "every file at every save",
            CheckStep::Picked => "picked files",
            CheckStep::Solids => "solid copies",
        };
        println!("{}", colors::dim(&format!("  checking {}…", what)));
    })?;

    println!(
        "Checked {}, {}, {}, {} and {}.",
        colors::count(report.objects as u64, "stored piece", "stored pieces"),
        colors::count(report.saves as u64, "save", "saves"),
        colors::count(report.file_versions as u64, "file version", "file versions"),
        colors::count(report.picked as u64, "picked file", "picked files"),
        colors::count(report.solids as u64, "solid copy", "solid copies"),
    );

    if report.is_intact() {
        println!("{}", colors::green("Everything is intact: every save reads back exactly as it was saved."));
        return Ok(());
    }
    for problem in &report.problems {
        println!("  {} {}", colors::red("✗"), problem);
    }
    Err(format!("found {}", colors::count(report.problems.len() as u64, "problem", "problems")).into())
}
