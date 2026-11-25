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
        "Checked {} stored piece(s), {} save(s), {} file version(s), {} picked file(s) and {} solid cop{}.",
        report.objects,
        report.saves,
        report.file_versions,
        report.picked,
        report.solids,
        if report.solids == 1 { "y" } else { "ies" }
    );

    if report.is_intact() {
        println!("{}", colors::green("Everything is intact: every save reads back exactly as it was saved."));
        return Ok(());
    }
    for problem in &report.problems {
        println!("  {} {}", colors::red("✗"), problem);
    }
    Err(format!("found {} problem(s)", report.problems.len()).into())
}
