//! Small ANSI color helpers for the `duh` CLI.
//!
//! Behavior:
//! - Colors are disabled when the `NO_COLOR` environment variable is present.
//! - Colors are disabled when stdout is not a TTY.

use atty::Stream;
use std::env;

fn enabled() -> bool {
    // Respect NO_COLOR (presence disables color)
    if env::var_os("NO_COLOR").is_some() {
        return false;
    }
    // Only enable colors when stdout is a TTY
    atty::is(Stream::Stdout)
}

fn wrap(code: &str, s: &str) -> String {
    if !enabled() {
        s.to_string()
    } else {
        format!("\x1b[{}m{}\x1b[0m", code, s)
    }
}

pub fn bold(s: &str) -> String {
    wrap("1", s)
}

pub fn green(s: &str) -> String {
    wrap("32", s)
}

pub fn red(s: &str) -> String {
    wrap("31", s)
}

pub fn yellow(s: &str) -> String {
    wrap("33", s)
}

pub fn cyan(s: &str) -> String {
    wrap("36", s)
}

pub fn dim(s: &str) -> String {
    wrap("2", s)
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
