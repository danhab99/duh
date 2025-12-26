use clap::{self, Parser};
use std::fs::File;

/// Simple program to greet a person
#[derive(clap::Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    pub original: String,

    #[arg(short, long)]
    pub next: String,

    #[arg(short, long)]
    pub blocksize: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let original_bytes = File::open(args.original)?;
    let next_bytes = File::open(args.next)?;

    let fragments = lib::diff::build_diff_fragments(original_bytes, next_bytes, args.blocksize)?;
    
    println!("Diff fragments:");
    for fragment in fragments {
        println!("{}", fragment);
    }

    Ok(())
}
