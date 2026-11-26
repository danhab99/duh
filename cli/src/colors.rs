//! Small ANSI color helpers for the `duh` CLI.
//!
//! Behavior:
//! - Colors are disabled when the `NO_COLOR` environment variable is present.
//! - Colors are disabled when stdout is not a TTY.

use atty::Stream;
use std::env;

pub fn enabled() -> bool {
    // Respect NO_COLOR (presence disables color)
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // Only enable colors when stdout is a TTY
    atty::is(Stream::Stdout)
}

/// `s` without its ANSI color codes when color is off. For text colored
/// elsewhere (the change bars come from the library already colored).
pub fn plain(s: &str) -> String {
    if enabled() {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip `ESC [ … m`.
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn wrap(code: &str, s: &str) -> String {
    if !enabled() {
        s.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    }
}

pub const BOLD: &str = "1";
pub fn bold(s: &str) -> String {
    wrap(BOLD, s)
}

pub const GREEN: &str = "32";
pub fn green(s: &str) -> String {
    wrap(GREEN, s)
}

pub const RED: &str = "31";
pub fn red(s: &str) -> String {
    wrap(RED, s)
}

pub const YELLOW: &str = "33";
pub fn yellow(s: &str) -> String {
    wrap(YELLOW, s)
}

pub const CYAN: &str = "36";
pub fn cyan(s: &str) -> String {
    wrap(CYAN, s)
}

pub const DIM: &str = "2";
pub fn dim(s: &str) -> String {
    wrap(DIM, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn no_color_env_disables_colors() {
        env::set_var("NO_COLOR", "1");
        assert_eq!(green("x"), "x");
        env::remove_var("NO_COLOR");
    }

    #[test]
    fn returns_plain_text_when_stdout_not_tty() {
        if !atty::is(Stream::Stdout) {
            assert_eq!(bold("a"), "a");
        }
    }
}

/// "1 save", "3 saves": a number with its noun in the right form.
pub fn count(n: impl Into<u64>, one: &str, many: &str) -> String {
    let n = n.into();
    format!("{} {}", n, if n == 1 { one } else { many })
}
