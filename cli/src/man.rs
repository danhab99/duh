//! Manual pages: `duh(1)` and one `duh-<command>(1)` per command, made from
//! the same definitions as `--help`, so the two can't disagree. Nix writes
//! them at install time with `duh __man <dir>`.

use std::fs;
use std::io::Write;
use std::path::Path;

use clap::CommandFactory;
use clap_mangen::Man;

use crate::cli::Cli;

/// The parts of duh(1) that `--help` doesn't have, in roff.
const MAIN_PAGE_EXTRA: &str = r#".SH DESCRIPTION
duh keeps every version of your files: 3D models, images, audio and video
projects, anything. Each save stores only the bytes that changed, and every
version comes back exactly, byte for byte.
.PP
A folder whose versions duh keeps is a \fIspace\fR; the versions live in
its hidden \fI.duh\fR folder. You \fBpick\fR files for the next save, then
\fBsave\fR them with a sentence about what changed. Saves form a line
called a \fIdraft\fR (the first is \fBmain\fR). A \fIlabel\fR names one
save. A \fIplace\fR is another copy of the space (a folder, a drive, cloud
storage) that you \fBsend\fR saves to and \fBreceive\fR them from.
.PP
Every command also answers to its git name, shown in brackets below.
duh never merges: sending and receiving only ever move a draft forward.
.SH VERSIONS
Wherever a command takes a version, any of these work:
.TP
\fIfinal-v2\fR
the save with that label
.TP
\fImain\fR
the latest save on that draft
.TP
\fI9298ff3b\fR
the save whose hash starts with that (any unique prefix)
.TP
\fBHEAD\fR
your latest save on the open draft
.TP
\fBHEAD~\fIN\fR, \fIX\fB^\fR
\fIN\fR saves before \fBHEAD\fR; the save before \fIX\fR
.SH FILES
.TP
\fI.duh/\fR
the space: every version, its settings (\fI.duh/config\fR, TOML) and its
journal. Credentials for places are stored here in plain text.
.TP
\fI.duhignore\fR
patterns, one per line, for files \fBlook\fR and \fBpick .\fR should skip,
as in \fI.gitignore\fR: \fB*.bak\fR, \fBrenders/\fR, \fB/top-only.txt\fR,
\fB!keep.bak\fR, \fB#\fR comments. Files and folders whose names start with
a dot are always skipped unless named.
.SH ENVIRONMENT
.TP
\fBEDITOR\fR
the editor \fBduh save\fR opens for the message when none is given; when
unset, the message is written from what changed
.TP
\fBNO_COLOR\fR
when set, never use colour
.TP
\fBUSER\fR, \fBUSERNAME\fR
the name new spaces put on saves until \fBuser.name\fR is set
.SH EXIT STATUS
0 on success, 1 when a command fails (the reason is printed on standard
error), 2 when the command line itself is wrong.
.SH EXAMPLES
.nf
duh start
duh pick .
duh save \-m "First rough shape"
duh compare
duh save \-a \-m "Thicker walls"
duh history \-\-oneline
duh bring\-back body.FCStd \-\-from HEAD~1 \-\-as body\-before.FCStd
duh open \-\-new snap\-fit
duh places add usb /media/usb/bracket
duh send usb
duh check usb
.fi
"#;

const SEE_ALSO_MAIN: &str = r#".SH SEE ALSO
\fBduh\-<command>\fR(1) for each command, for example \fBduh\-save\fR(1).
.PP
The guides, starting with "Getting started":
https://github.com/danhab99/duh/tree/main/docs
"#;

/// Write duh.1 and duh-<command>.1 for every command into `dir`.
pub fn write(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut cmd = Cli::command().disable_help_subcommand(true);
    cmd.build();
    let version = cmd.get_version().unwrap_or_default().to_string();

    // duh(1): clap's title, name, synopsis and commands, then ours.
    let main = Man::new(cmd.clone()).manual("duh manual").source(format!("duh {}", version));
    let mut page = Vec::new();
    main.render_title(&mut page)?;
    main.render_name_section(&mut page)?;
    main.render_synopsis_section(&mut page)?;
    page.write_all(MAIN_PAGE_EXTRA.as_bytes())?;
    main.render_subcommands_section(&mut page)?;
    page.write_all(SEE_ALSO_MAIN.as_bytes())?;
    fs::write(dir.join("duh.1"), page)?;

    // duh-<command>(1), and duh-places-<action>(1) and so on.
    fn sub(cmd: &clap::Command, version: &str, dir: &Path) -> std::io::Result<()> {
        for c in cmd.get_subcommands().filter(|c| !c.is_hide_set()) {
            let man = Man::new(c.clone()).manual("duh manual").source(format!("duh {}", version));
            let mut page = Vec::new();
            man.render(&mut page)?;
            page.write_all(
                format!(".SH SEE ALSO\n\\fBduh\\fR(1), and https://github.com/danhab99/duh/tree/main/docs\n").as_bytes(),
            )?;
            fs::write(dir.join(man.get_filename()), page)?;
            sub(c, version, dir)?;
        }
        Ok(())
    }
    sub(&cmd, &version, dir)
}
