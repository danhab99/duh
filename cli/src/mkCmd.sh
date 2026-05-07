# Check if the directory is passed as an argument
if [ -z "$1" ]; then
  echo "Usage: $0 <directory>"
  exit 1
fi

cat <<EOF > "$1.rs"
use std::error::Error;

use clap::clap_derive::Args;
use lib::objects::{Object, ObjectReference};
use lib::repo::Repo;

/// Show the commit currently referenced by HEAD
#[derive(Args)]
#[command(about = "")]
pub struct $1Command {
}

pub fn $1(repo: &mut Repo, cmd: &$1Command) -> Result<(), Box<dyn Error>> {
}

EOF

