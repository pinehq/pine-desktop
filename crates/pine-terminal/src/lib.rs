//! Toolkit-independent contract for terminal implementations.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// VTE makes the GTK MVP usable. `libghostty-vt` is the target after Pine owns a renderer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    VteMvp,
    LibghosttyVt,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalLaunch {
    cwd: PathBuf,
    executable: PathBuf,
    arguments: Vec<String>,
}

impl TerminalLaunch {
    /// # Errors
    ///
    /// Returns an error when the working directory, executable, or arguments are invalid.
    pub fn new(
        cwd: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        arguments: Vec<String>,
    ) -> Result<Self, LaunchError> {
        let launch = Self {
            cwd: cwd.into(),
            executable: executable.into(),
            arguments,
        };
        launch.validate()?;
        Ok(launch)
    }

    /// # Errors
    ///
    /// Returns an error when `cwd` is not absolute.
    pub fn user_shell(cwd: impl Into<PathBuf>) -> Result<Self, LaunchError> {
        let executable = std::env::var_os("SHELL")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from);
        Self::new(cwd, executable, Vec::new())
    }

    #[must_use]
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }

    fn validate(&self) -> Result<(), LaunchError> {
        if !self.cwd.is_absolute() {
            return Err(LaunchError::RelativeWorkingDirectory);
        }
        if self.executable.as_os_str().is_empty() {
            return Err(LaunchError::EmptyExecutable);
        }
        if self
            .arguments
            .iter()
            .any(|argument| argument.contains('\0'))
        {
            return Err(LaunchError::NulArgument);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchError {
    RelativeWorkingDirectory,
    EmptyExecutable,
    NulArgument,
}

impl fmt::Display for LaunchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::RelativeWorkingDirectory => "terminal working directory must be absolute",
            Self::EmptyExecutable => "terminal executable must not be empty",
            Self::NulArgument => "terminal argument contains a NUL byte",
        };
        formatter.write_str(message)
    }
}

impl Error for LaunchError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_is_structured_without_shell_interpolation() {
        let launch = TerminalLaunch::new(
            "/tmp/pine project",
            "/usr/bin/codex",
            vec!["--model".into(), "gpt-5".into()],
        )
        .unwrap();

        assert_eq!(launch.executable(), Path::new("/usr/bin/codex"));
        assert_eq!(launch.arguments()[0], "--model");
    }

    #[test]
    fn relative_working_directory_is_rejected() {
        assert_eq!(
            TerminalLaunch::new("relative", "/bin/sh", Vec::new()).unwrap_err(),
            LaunchError::RelativeWorkingDirectory
        );
    }
}
