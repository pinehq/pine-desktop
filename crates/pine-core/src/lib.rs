//! Portable product model shared by native Pine frontends.

mod task;

pub use task::{AgentKind, AgentTask, RegistryError, TaskId, TaskRegistry, TaskState};
