//! Toolkit-independent Git status and diff services.

use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
const MAX_ERROR_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeKind {
    Unmodified,
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    Unmerged,
    Untracked,
    Ignored,
    Unknown,
}

impl ChangeKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Unmodified => "Unmodified",
            Self::Added => "Added",
            Self::Modified => "Modified",
            Self::Deleted => "Deleted",
            Self::Renamed => "Renamed",
            Self::Copied => "Copied",
            Self::Unmerged => "Conflict",
            Self::Untracked => "Untracked",
            Self::Ignored => "Ignored",
            Self::Unknown => "Changed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileStatus {
    path: PathBuf,
    original_path: Option<PathBuf>,
    index: ChangeKind,
    worktree: ChangeKind,
}

impl FileStatus {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn original_path(&self) -> Option<&Path> {
        self.original_path.as_deref()
    }

    #[must_use]
    pub const fn index(&self) -> ChangeKind {
        self.index
    }

    #[must_use]
    pub const fn worktree(&self) -> ChangeKind {
        self.worktree
    }

    #[must_use]
    pub const fn summary(&self) -> &'static str {
        if matches!(self.worktree, ChangeKind::Unmodified) {
            self.index.label()
        } else {
            self.worktree.label()
        }
    }

    #[must_use]
    pub const fn is_untracked(&self) -> bool {
        matches!(self.index, ChangeKind::Untracked)
            || matches!(self.worktree, ChangeKind::Untracked)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StatusSnapshot {
    branch: Option<String>,
    files: Vec<FileStatus>,
}

impl StatusSnapshot {
    #[must_use]
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    #[must_use]
    pub fn files(&self) -> &[FileStatus] {
        &self.files
    }
}

#[derive(Clone, Debug)]
pub struct Repository {
    root: PathBuf,
    scope: PathBuf,
}

impl Repository {
    /// Opens a repository rooted at or above `project_root`.
    ///
    /// # Errors
    ///
    /// Returns an error if Git is unavailable, `project_root` is not in a
    /// repository, or Git returns an invalid root path.
    pub fn discover(project_root: &Path) -> Result<Self, GitError> {
        let project_root = project_root
            .canonicalize()
            .map_err(|source| GitError::ReadFile {
                path: project_root.to_path_buf(),
                source,
            })?;
        let output = run_git(
            &project_root,
            ["rev-parse", "--show-toplevel"],
            "discover repository",
        )?;
        let root = String::from_utf8(output.stdout)
            .map_err(|_| GitError::InvalidOutput("repository root is not UTF-8"))?;
        let root = PathBuf::from(root.trim_end());
        if !root.is_absolute() {
            return Err(GitError::InvalidOutput("repository root is not absolute"));
        }
        let root = root
            .canonicalize()
            .map_err(|source| GitError::ReadFile { path: root, source })?;
        let scope = project_root
            .strip_prefix(&root)
            .map_err(|_| GitError::PathOutsideRepository(project_root.clone()))?
            .to_path_buf();
        Ok(Self { root, scope })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads branch and porcelain status without changing repository state.
    ///
    /// # Errors
    ///
    /// Returns an error if Git fails or emits malformed status data.
    pub fn status(&self) -> Result<StatusSnapshot, GitError> {
        let branch_output = run_git(
            &self.root,
            ["branch", "--show-current"],
            "read current branch",
        )?;
        let branch = String::from_utf8(branch_output.stdout)
            .map_err(|_| GitError::InvalidOutput("branch name is not UTF-8"))?;
        let branch = match branch.trim_end() {
            "" => None,
            value => Some(value.to_owned()),
        };

        let mut status_command = git_command(&self.root);
        status_command.args(["status", "--porcelain=v1", "-z", "--untracked-files=all"]);
        if !self.scope.as_os_str().is_empty() {
            status_command.arg("--").arg(&self.scope);
        }
        let status_output = status_command.output().map_err(|source| GitError::Launch {
            operation: "read status",
            source,
        })?;
        if !status_output.status.success() {
            return Err(command_failed(&status_output, "read status"));
        }
        let files = parse_porcelain_v1_z(&status_output.stdout)?;
        Ok(StatusSnapshot { branch, files })
    }

    /// Produces a bounded, no-color diff for one status entry.
    ///
    /// # Errors
    ///
    /// Returns an error if the path escapes the repository, the file cannot be
    /// read, Git fails, or the diff exceeds the preview limit.
    pub fn diff(&self, status: &FileStatus) -> Result<String, GitError> {
        if status.is_untracked() {
            return self.untracked_diff(status.path());
        }

        let output = git_command(&self.root)
            .args(["diff", "--no-ext-diff", "--no-color", "HEAD", "--"])
            .arg(status.path())
            .output()
            .map_err(|source| GitError::Launch {
                operation: "read diff",
                source,
            })?;
        checked_stdout(output, "read diff", MAX_DIFF_BYTES)
    }

    fn untracked_diff(&self, relative_path: &Path) -> Result<String, GitError> {
        if relative_path.is_absolute()
            || relative_path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(GitError::PathOutsideRepository(relative_path.to_path_buf()));
        }

        let path = self.root.join(relative_path);
        let canonical = path.canonicalize().map_err(|source| GitError::ReadFile {
            path: path.clone(),
            source,
        })?;
        let canonical_root = self
            .root
            .canonicalize()
            .map_err(|source| GitError::ReadFile {
                path: self.root.clone(),
                source,
            })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(GitError::PathOutsideRepository(relative_path.to_path_buf()));
        }

        let file = File::open(&canonical).map_err(|source| GitError::ReadFile {
            path: canonical.clone(),
            source,
        })?;
        let mut bytes = Vec::new();
        file.take((MAX_DIFF_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|source| GitError::ReadFile {
                path: canonical,
                source,
            })?;
        if bytes.len() > MAX_DIFF_BYTES {
            return Err(GitError::OutputTooLarge {
                operation: "read untracked diff",
                limit: MAX_DIFF_BYTES,
            });
        }
        let text = String::from_utf8(bytes)
            .map_err(|_| GitError::InvalidOutput("untracked file is not UTF-8"))?;
        let mut diff = format!(
            "diff --git a/{0} b/{0}\nnew file mode 100644\n--- /dev/null\n+++ b/{0}\n",
            relative_path.display()
        );
        for (index, line) in text.split_inclusive('\n').enumerate() {
            use fmt::Write as _;
            if index == 0 {
                let line_count = text.lines().count().max(1);
                let _ = writeln!(diff, "@@ -0,0 +1,{line_count} @@");
            }
            diff.push('+');
            diff.push_str(line);
        }
        if !text.is_empty() && !text.ends_with('\n') {
            diff.push_str("\n\\ No newline at end of file\n");
        }
        Ok(diff)
    }
}

#[derive(Debug)]
pub enum GitError {
    Launch {
        operation: &'static str,
        source: io::Error,
    },
    CommandFailed {
        operation: &'static str,
        status: Option<i32>,
        stderr: String,
    },
    InvalidOutput(&'static str),
    OutputTooLarge {
        operation: &'static str,
        limit: usize,
    },
    PathOutsideRepository(PathBuf),
    ReadFile {
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launch { operation, source } => {
                write!(formatter, "unable to {operation}: {source}")
            }
            Self::CommandFailed {
                operation,
                status,
                stderr,
            } => write!(
                formatter,
                "unable to {operation}: git exited with {}{}",
                status.map_or_else(|| "a signal".to_owned(), |code| code.to_string()),
                if stderr.is_empty() {
                    String::new()
                } else {
                    format!(": {stderr}")
                }
            ),
            Self::InvalidOutput(message) => write!(formatter, "invalid Git output: {message}"),
            Self::OutputTooLarge { operation, limit } => {
                write!(
                    formatter,
                    "unable to {operation}: output exceeds {limit} bytes"
                )
            }
            Self::PathOutsideRepository(path) => write!(
                formatter,
                "path is outside the repository: {}",
                path.display()
            ),
            Self::ReadFile { path, source } => {
                write!(formatter, "unable to read {}: {source}", path.display())
            }
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Launch { source, .. } | Self::ReadFile { source, .. } => Some(source),
            _ => None,
        }
    }
}

fn run_git<const N: usize>(
    root: &Path,
    arguments: [&str; N],
    operation: &'static str,
) -> Result<Output, GitError> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(|source| GitError::Launch { operation, source })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(command_failed(&output, operation))
    }
}

fn git_command(root: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("--literal-pathspecs")
        .arg("-C")
        .arg(root);
    command
}

fn checked_stdout(
    output: Output,
    operation: &'static str,
    limit: usize,
) -> Result<String, GitError> {
    if !output.status.success() {
        return Err(command_failed(&output, operation));
    }
    if output.stdout.len() > limit {
        return Err(GitError::OutputTooLarge { operation, limit });
    }
    String::from_utf8(output.stdout)
        .map_err(|_| GitError::InvalidOutput("command output is not UTF-8"))
}

fn command_failed(output: &Output, operation: &'static str) -> GitError {
    let stderr =
        String::from_utf8_lossy(&output.stderr[..output.stderr.len().min(MAX_ERROR_BYTES)])
            .trim()
            .to_owned();
    GitError::CommandFailed {
        operation,
        status: output.status.code(),
        stderr,
    }
}

fn parse_porcelain_v1_z(output: &[u8]) -> Result<Vec<FileStatus>, GitError> {
    let mut fields = output.split(|byte| *byte == 0).peekable();
    let mut statuses = Vec::new();

    while let Some(entry) = fields.next() {
        if entry.is_empty() {
            if fields.peek().is_none() {
                break;
            }
            return Err(GitError::InvalidOutput("empty status entry"));
        }
        if entry.len() < 4 || entry[2] != b' ' {
            return Err(GitError::InvalidOutput("malformed status entry"));
        }
        let index = parse_kind(entry[0]);
        let worktree = parse_kind(entry[1]);
        let path = parse_path(&entry[3..])?;
        let is_rename_or_copy = matches!(entry[0], b'R' | b'C') || matches!(entry[1], b'R' | b'C');
        let original_path = if is_rename_or_copy {
            let field = fields
                .next()
                .ok_or(GitError::InvalidOutput("rename source is missing"))?;
            Some(parse_path(field)?)
        } else {
            None
        };
        statuses.push(FileStatus {
            path,
            original_path,
            index,
            worktree,
        });
    }

    Ok(statuses)
}

fn parse_path(bytes: &[u8]) -> Result<PathBuf, GitError> {
    if bytes.is_empty() {
        return Err(GitError::InvalidOutput("status path is empty"));
    }
    let path = std::str::from_utf8(bytes)
        .map_err(|_| GitError::InvalidOutput("status path is not UTF-8"))?;
    Ok(PathBuf::from(path))
}

const fn parse_kind(code: u8) -> ChangeKind {
    match code {
        b' ' => ChangeKind::Unmodified,
        b'A' => ChangeKind::Added,
        b'M' => ChangeKind::Modified,
        b'D' => ChangeKind::Deleted,
        b'R' => ChangeKind::Renamed,
        b'C' => ChangeKind::Copied,
        b'U' => ChangeKind::Unmerged,
        b'?' => ChangeKind::Untracked,
        b'!' => ChangeKind::Ignored,
        _ => ChangeKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn parses_modified_untracked_and_renamed_entries() {
        let output = b" M src/main.rs\0?? notes.txt\0R  new.rs\0old.rs\0";
        let statuses = parse_porcelain_v1_z(output).unwrap();

        assert_eq!(statuses.len(), 3);
        assert_eq!(statuses[0].path(), Path::new("src/main.rs"));
        assert_eq!(statuses[0].worktree(), ChangeKind::Modified);
        assert!(statuses[1].is_untracked());
        assert_eq!(statuses[2].path(), Path::new("new.rs"));
        assert_eq!(statuses[2].original_path(), Some(Path::new("old.rs")));
        assert_eq!(statuses[2].summary(), "Renamed");
    }

    #[test]
    fn rejects_missing_rename_source() {
        let error = parse_porcelain_v1_z(b"R  new.rs\0").unwrap_err();
        assert!(matches!(error, GitError::InvalidOutput(_)));
    }

    #[test]
    fn rejects_malformed_entries() {
        let error = parse_porcelain_v1_z(b"M file.rs\0").unwrap_err();
        assert!(matches!(error, GitError::InvalidOutput(_)));
    }

    #[test]
    fn reads_status_and_diff_from_a_repository() {
        let repository = TestRepository::new();
        repository.write("tracked.txt", "initial\n");
        repository.git(["add", "tracked.txt"]);
        repository.git([
            "-c",
            "user.name=Pine Tests",
            "-c",
            "user.email=pine@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "initial",
        ]);
        repository.write("tracked.txt", "changed\n");
        repository.write("untracked.txt", "new\n");

        let git = Repository::discover(&repository.path).unwrap();
        let snapshot = git.status().unwrap();
        let tracked = snapshot
            .files()
            .iter()
            .find(|file| file.path() == Path::new("tracked.txt"))
            .unwrap();
        let untracked = snapshot
            .files()
            .iter()
            .find(|file| file.path() == Path::new("untracked.txt"))
            .unwrap();

        assert_eq!(tracked.worktree(), ChangeKind::Modified);
        assert!(git.diff(tracked).unwrap().contains("+changed"));
        assert!(untracked.is_untracked());
        assert!(git.diff(untracked).unwrap().contains("+new"));
    }

    #[test]
    fn scopes_status_to_the_opened_project_directory() {
        let repository = TestRepository::new();
        repository.write("project/inside.txt", "initial\n");
        repository.write("outside.txt", "initial\n");
        repository.git(["add", "."]);
        repository.git([
            "-c",
            "user.name=Pine Tests",
            "-c",
            "user.email=pine@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "initial",
        ]);
        repository.write("project/inside.txt", "changed\n");
        repository.write("outside.txt", "changed\n");

        let git = Repository::discover(&repository.path.join("project")).unwrap();
        let snapshot = git.status().unwrap();

        assert_eq!(snapshot.files().len(), 1);
        assert_eq!(snapshot.files()[0].path(), Path::new("project/inside.txt"));
    }

    struct TestRepository {
        path: PathBuf,
    }

    impl TestRepository {
        fn new() -> Self {
            static NEXT_ID: AtomicU64 = AtomicU64::new(0);
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("pine-git-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).unwrap();
            let repository = Self { path };
            repository.git(["init", "--quiet"]);
            repository
        }

        fn write(&self, relative_path: &str, contents: &str) {
            let path = self.path.join(relative_path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, contents).unwrap();
        }

        fn git<const N: usize>(&self, arguments: [&str; N]) {
            let output = Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(arguments)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    impl Drop for TestRepository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
