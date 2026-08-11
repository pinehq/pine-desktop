use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::error::Error;
use std::fmt;

/// Stable identity of one user intent. A resumed process does not get a new task id.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TaskId(u64);

impl TaskId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "task-{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentKind {
    Codex,
    ClaudeCode,
    Pi,
    Generic,
}

impl fmt::Display for AgentKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Codex => "Codex",
            Self::ClaudeCode => "Claude Code",
            Self::Pi => "Pi",
            Self::Generic => "CLI agent",
        };
        formatter.write_str(label)
    }
}

/// Task state deliberately separates process exit from verified completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Draft,
    Working,
    WaitingForUser,
    ExitedUnknown,
    CompletedVerified,
    FailedVerified,
    Paused,
    Canceled,
}

impl TaskState {
    #[must_use]
    pub const fn can_transition_to(self, next: Self) -> bool {
        use TaskState::{
            Canceled, CompletedVerified, Draft, ExitedUnknown, FailedVerified, Paused,
            WaitingForUser, Working,
        };

        matches!(
            (self, next),
            (Draft | Paused, Working | Canceled)
                | (Working, WaitingForUser | ExitedUnknown | Paused | Canceled)
                | (WaitingForUser, Working | Paused | Canceled)
                | (
                    ExitedUnknown,
                    Working | CompletedVerified | FailedVerified | Canceled
                )
        )
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Working => "Working",
            Self::WaitingForUser => "Needs attention",
            Self::ExitedUnknown => "Exited — unverified",
            Self::CompletedVerified => "Completed",
            Self::FailedVerified => "Failed",
            Self::Paused => "Paused",
            Self::Canceled => "Canceled",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentTask {
    id: TaskId,
    title: String,
    agent: AgentKind,
    state: TaskState,
}

impl AgentTask {
    #[must_use]
    pub fn new(id: TaskId, title: impl Into<String>, agent: AgentKind) -> Self {
        Self {
            id,
            title: title.into(),
            agent,
            state: TaskState::Draft,
        }
    }

    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn agent(&self) -> AgentKind {
        self.agent
    }

    #[must_use]
    pub const fn state(&self) -> TaskState {
        self.state
    }

    fn transition(&mut self, next: TaskState) -> Result<(), RegistryError> {
        if !self.state.can_transition_to(next) {
            return Err(RegistryError::InvalidTransition {
                id: self.id,
                from: self.state,
                to: next,
            });
        }
        self.state = next;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct TaskRegistry {
    tasks: HashMap<TaskId, AgentTask>,
}

impl TaskRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// # Errors
    ///
    /// Returns [`RegistryError::DuplicateTask`] when the id is already registered.
    pub fn insert(&mut self, task: AgentTask) -> Result<(), RegistryError> {
        let id = task.id();
        match self.tasks.entry(id) {
            Entry::Occupied(_) => Err(RegistryError::DuplicateTask(id)),
            Entry::Vacant(entry) => {
                entry.insert(task);
                Ok(())
            }
        }
    }

    /// # Errors
    ///
    /// Returns an error when the task is unknown or the state transition is invalid.
    pub fn transition(&mut self, id: TaskId, next: TaskState) -> Result<(), RegistryError> {
        self.tasks
            .get_mut(&id)
            .ok_or(RegistryError::UnknownTask(id))?
            .transition(next)
    }

    #[must_use]
    pub fn get(&self, id: TaskId) -> Option<&AgentTask> {
        self.tasks.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AgentTask> {
        self.tasks.values()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryError {
    DuplicateTask(TaskId),
    UnknownTask(TaskId),
    InvalidTransition {
        id: TaskId,
        from: TaskState,
        to: TaskState,
    },
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateTask(id) => write!(formatter, "duplicate task {id}"),
            Self::UnknownTask(id) => write!(formatter, "unknown task {id}"),
            Self::InvalidTransition { id, from, to } => {
                write!(formatter, "invalid transition for {id}: {from:?} -> {to:?}")
            }
        }
    }
}

impl Error for RegistryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_exit_is_not_completion() {
        let id = TaskId::new(7);
        let mut registry = TaskRegistry::new();
        registry
            .insert(AgentTask::new(id, "Build Linux UI", AgentKind::Codex))
            .unwrap();

        registry.transition(id, TaskState::Working).unwrap();
        registry.transition(id, TaskState::ExitedUnknown).unwrap();

        assert_eq!(registry.get(id).unwrap().state(), TaskState::ExitedUnknown);
    }

    #[test]
    fn completion_requires_exited_unknown() {
        let id = TaskId::new(8);
        let mut registry = TaskRegistry::new();
        registry
            .insert(AgentTask::new(id, "Review patch", AgentKind::Generic))
            .unwrap();
        registry.transition(id, TaskState::Working).unwrap();

        assert!(
            registry
                .transition(id, TaskState::CompletedVerified)
                .is_err()
        );
    }

    #[test]
    fn duplicate_identity_is_rejected_without_replacing_original() {
        let id = TaskId::new(9);
        let mut registry = TaskRegistry::new();
        registry
            .insert(AgentTask::new(id, "Original", AgentKind::Codex))
            .unwrap();

        assert!(
            registry
                .insert(AgentTask::new(id, "Replacement", AgentKind::Generic))
                .is_err()
        );
        assert_eq!(registry.get(id).unwrap().title(), "Original");
    }
}
