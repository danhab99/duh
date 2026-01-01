use clap::{self, Parser};
use std::{
    fs::{self, File},
    os::unix::fs::MetadataExt,
};

/// Demo of CDC-based diff fragment detection
#[derive(clap::Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    pub original: String,

    #[arg(short, long)]
    pub next: String,

    #[arg(short, long)]
    pub window: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let original_meta = fs::metadata(&args.original)?;
    let new_meta = fs::metadata(&args.next)?;

    let original_bytes = File::open(&args.original)?;
    let next_bytes = File::open(&args.next)?;

    println!("Building diff fragments between:");
    println!("  Old: {}", args.original);
    println!("  New: {}", args.next);
    println!();

    let mut fragments = Vec::new();
    let mut total_added = 0;
    let mut total_unchanged = 0;
    let mut total_deleted = 0;
    let mut count = 0;

    println!("Diff fragments:");
    println!("{:-<60}", "");

    for frag_result in lib::diff::build_diff_fragments(original_bytes, next_bytes, args.window) {
        let fragment = frag_result?;
        count += 1;
        
        match &fragment {
            lib::diff::DiffFragment::ADDED { body } => {
                println!("{:4}. ADDED      {} bytes", count, body.len());
                total_added += body.len();
            }
            lib::diff::DiffFragment::UNCHANGED { len } => {
                println!("{:4}. UNCHANGED  {} bytes", count, len);
                total_unchanged += len;
            }
            lib::diff::DiffFragment::DELETED { len } => {
                println!("{:4}. DELETED    {} bytes", count, len);
                total_deleted += len;
            }
        }
        
        fragments.push(fragment);
    }

    println!("{:-<60}", "");
    println!("Summary:");
    println!("  Original file: {} bytes", original_meta.size());
    println!("  New file:      {} bytes", new_meta.size());
    println!();
    println!("  Added:     {} bytes", total_added);
    println!("  Unchanged: {} bytes", total_unchanged);
    println!("  Deleted:   {} bytes", total_deleted);
    println!("  Total diff size: {} fragments", count);

    Ok(())
}
