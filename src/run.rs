use std::process::Command;

/// The captured result of running a subprocess.
pub struct Output {
    pub status: i32,
    pub stdout: String,
    // Read by later subcommands that surface a failed subprocess's diagnostics.
    #[allow(dead_code)]
    pub stderr: String,
}

/// The injectable subprocess seam. Every subcommand shells out through this so
/// tests can substitute a fake that records argv instead of spawning.
pub trait Runner: Send + Sync {
    /// Run `argv` with an optional env overlay applied on top of the inherited
    /// environment. In each `env` entry, `Some(v)` sets the var and `None`
    /// UNSETS it (the `HERDR_PANE_ID` scrub later subcommands rely on).
    fn run(&self, argv: &[&str], env: &[(&str, Option<&str>)]) -> std::io::Result<Output>;
}

pub struct RealRunner;

impl Runner for RealRunner {
    fn run(&self, argv: &[&str], env: &[(&str, Option<&str>)]) -> std::io::Result<Output> {
        let mut cmd = Command::new(argv[0]);
        cmd.args(&argv[1..]);
        for (key, val) in env {
            match val {
                Some(v) => {
                    cmd.env(key, v);
                }
                None => {
                    cmd.env_remove(key);
                }
            }
        }
        let out = cmd.output()?;
        Ok(Output {
            status: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

/// The `rt` binary path: `RT_BIN_PATH` else `rt` on `PATH`.
pub fn rt_bin() -> String {
    std::env::var("RT_BIN_PATH").unwrap_or_else(|_| "rt".to_string())
}

/// The `herdr` binary path: `HERDR_BIN_PATH` else `herdr` on `PATH`.
// Consumed by the herdr subprocess calls a later task (herdr.rs) adds.
#[allow(dead_code)]
pub fn herdr_bin() -> String {
    std::env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string())
}

/// The `deck` binary path: `DECK_BIN_PATH` else `deck` on `PATH`.
pub fn deck_bin() -> String {
    std::env::var("DECK_BIN_PATH").unwrap_or_else(|_| "deck".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_sets_a_var() {
        let r = RealRunner;
        let out = r
            .run(
                &["/bin/sh", "-c", "printf '%s' \"$HERDR_OVERLAY_SET\""],
                &[("HERDR_OVERLAY_SET", Some("yes"))],
            )
            .unwrap();
        assert_eq!(out.status, 0);
        assert_eq!(out.stdout, "yes");
    }

    #[test]
    fn overlay_none_unsets_a_var() {
        // HOME is inherited from the parent env and `sh` does not repopulate it
        // (unlike PATH); `${HOME+SET}` prints SET only while it is set. The None
        // overlay must fully remove it.
        let r = RealRunner;
        let with = r
            .run(&["/bin/sh", "-c", "printf '%s' \"${HOME+SET}\""], &[])
            .unwrap();
        assert_eq!(with.stdout, "SET");

        let scrubbed = r
            .run(
                &["/bin/sh", "-c", "printf '%s' \"${HOME+SET}\""],
                &[("HOME", None)],
            )
            .unwrap();
        assert_eq!(scrubbed.stdout, "");
    }

    #[test]
    fn captures_status_and_stderr() {
        let r = RealRunner;
        let out = r
            .run(&["/bin/sh", "-c", "printf oops >&2; exit 3"], &[])
            .unwrap();
        assert_eq!(out.status, 3);
        assert_eq!(out.stderr, "oops");
    }

    #[test]
    fn bin_resolvers_default_to_bare_names() {
        // No env override in the test process, so the bare command name wins.
        assert_eq!(rt_bin(), "rt");
        assert_eq!(herdr_bin(), "herdr");
        assert_eq!(deck_bin(), "deck");
    }
}
