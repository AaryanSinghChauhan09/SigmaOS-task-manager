// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-task-manager — SigmaOS task-manager component inspired by Linux Mint / Omarchy / Arch Linux.
//!
//! Registry of named entries with fail-fast duplicate/missing handling.
//! Principles: single responsibility, fail-fast, defensive programming,
//! component isolation, stable API, deterministic tests.
#![forbid(unsafe_code)]

pub mod sigmataskmanager;

use std::fmt;
use std::collections::BTreeMap;

/// Errors for `RegistryError` operations. Fail-fast: invalid input is rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// An entry with this key is already registered
    Duplicate(String),
    /// No entry exists for the requested key
    NotFound(String),
    /// Registration key must not be empty
    EmptyKey,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(x) => write!(f, "an entry with this key is already registered: {x}"),
            Self::NotFound(x) => write!(f, "no entry exists for the requested key: {x}"),
            Self::EmptyKey => write!(f, "registration key must not be empty"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Named-entry registry (SRP: storage only; callers own semantics).
#[derive(Debug, Default)]
pub struct Registry {
    items: BTreeMap<String, String>,
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Self { Self::default() }

    /// Register `key -> value`. Fails on empty key or duplicate.
    pub fn register(&mut self, key: impl Into<String>, value: impl Into<String>) -> Result<(), RegistryError> {
        let key = key.into();
        if key.trim().is_empty() { return Err(RegistryError::EmptyKey); }
        if self.items.contains_key(&key) { return Err(RegistryError::Duplicate(key)); }
        self.items.insert(key, value.into());
        Ok(())
    }

    /// Look up a value by key.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.items.get(key).map(String::as_str)
    }

    /// Remove an entry; fails if absent.
    pub fn remove(&mut self, key: &str) -> Result<String, RegistryError> {
        self.items.remove(key).ok_or_else(|| RegistryError::NotFound(key.to_string()))
    }

    /// Number of entries.
    pub fn len(&self) -> usize { self.items.len() }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> Registry {
        let mut r = Registry::new();
        r.register("alpha", "one").unwrap();
        r.register("beta", "two").unwrap();
        r
    }

    #[test]
    fn register_and_get() {
        let r = reg();
        assert_eq!(r.get("alpha"), Some("one"));
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn duplicate_key_fails() {
        let mut r = Registry::new();
        r.register("x", "1").unwrap();
        assert!(matches!(r.register("x", "2"), Err(RegistryError::Duplicate(_))));
    }

    #[test]
    fn remove_missing_fails() {
        assert!(matches!(Registry::new().remove("nope"), Err(RegistryError::NotFound(_))));
    }

    #[test]
    fn empty_key_fails() {
        assert_eq!(Registry::new().register("  ", "v"), Err(RegistryError::EmptyKey));
    }

}

/// Backwards-compatible component handle (kept for existing callers/tests).
#[derive(Debug, Default)]
pub struct Component {
    name: &'static str,
}

impl Component {
    /// Create the component handle.
    pub fn new() -> Self {
        Self { name: "SigmaOS Component" }
    }

    /// Component label.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

// ---------------------------------------------------------------------------
// Task-manager service: manages a set of tasks with explicit lifecycle.
// ---------------------------------------------------------------------------

/// Errors for task-manager service operations. Fail-fast on invalid state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskManagerError {
    /// Service is already running
    AlreadyRunning,
    /// Service is not running
    NotRunning,
    /// Configuration is invalid
    InvalidConfig(&'static str),
    /// No task exists for the requested id
    TaskNotFound(u64),
}

impl fmt::Display for TaskManagerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => write!(f, "service is already running"),
            Self::NotRunning => write!(f, "service is not running"),
            Self::InvalidConfig(msg) => write!(f, "invalid configuration: {msg}"),
            Self::TaskNotFound(id) => write!(f, "no task exists for id {id}"),
        }
    }
}
impl std::error::Error for TaskManagerError {}

/// Task-manager configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum number of concurrently tracked tasks.
    pub max_tasks: usize,
    /// Whether the service records operational events.
    pub enable_logging: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self { max_tasks: 1024, enable_logging: true }
    }
}

impl Config {
    /// Validate and normalize configuration. Fail-fast on invalid values.
    pub fn process(&self) -> Result<(), TaskManagerError> {
        if self.max_tasks == 0 {
            return Err(TaskManagerError::InvalidConfig("max_tasks must be > 0"));
        }
        Ok(())
    }
}

/// A single managed task.
#[derive(Debug, Clone)]
pub struct Task {
    pub id: u64,
    pub name: String,
    pub running: bool,
}

/// Task-manager service: owns the task table and enforces lifecycle rules.
pub struct Service {
    config: Config,
    running: bool,
    next_task_id: u64,
    tasks: BTreeMap<u64, Task>,
}

impl Service {
    /// Create a service from validated configuration.
    pub fn new(config: Config) -> Self {
        Self { config, running: false, next_task_id: 1, tasks: BTreeMap::new() }
    }

    /// Whether the service is currently running.
    pub fn is_running(&self) -> bool { self.running }

    /// Number of tracked tasks.
    pub fn task_count(&self) -> usize { self.tasks.len() }

    /// Start the service. Fails fast if already running.
    pub fn start(&mut self) -> Result<(), TaskManagerError> {
        self.config.process()?;
        if self.running { return Err(TaskManagerError::AlreadyRunning); }
        self.running = true;
        Ok(())
    }

    /// Stop the service. Fails fast if not running.
    pub fn stop(&mut self) -> Result<(), TaskManagerError> {
        if !self.running { return Err(TaskManagerError::NotRunning); }
        self.running = false;
        Ok(())
    }

    /// Spawn a named task. Requires the service to be running.
    pub fn spawn(&mut self, name: impl Into<String>) -> Result<u64, TaskManagerError> {
        if !self.running { return Err(TaskManagerError::NotRunning); }
        if self.tasks.len() >= self.config.max_tasks {
            return Err(TaskManagerError::InvalidConfig("task table full"));
        }
        let id = self.next_task_id;
        self.next_task_id += 1;
        self.tasks.insert(id, Task { id, name: name.into(), running: true });
        Ok(id)
    }

    /// Stop a task by id. Requires the service to be running.
    pub fn kill(&mut self, id: u64) -> Result<(), TaskManagerError> {
        if !self.running { return Err(TaskManagerError::NotRunning); }
        match self.tasks.get_mut(&id) {
            Some(t) => { t.running = false; Ok(()) }
            None => Err(TaskManagerError::TaskNotFound(id)),
        }
    }
}

#[cfg(test)]
mod service_tests {
    use super::*;

    #[test]
    fn lifecycle_start_stop() {
        let mut s = Service::new(Config::default());
        assert!(!s.is_running());
        s.start().unwrap();
        assert!(s.is_running());
        s.stop().unwrap();
        assert!(!s.is_running());
    }

    #[test]
    fn double_start_fails() {
        let mut s = Service::new(Config::default());
        s.start().unwrap();
        assert_eq!(s.start(), Err(TaskManagerError::AlreadyRunning));
    }

    #[test]
    fn spawn_and_kill_task() {
        let mut s = Service::new(Config::default());
        s.start().unwrap();
        let id = s.spawn("worker").unwrap();
        assert_eq!(s.task_count(), 1);
        s.kill(id).unwrap();
        assert_eq!(s.tasks.get(&id).unwrap().running, false);
    }

    #[test]
    fn zero_max_tasks_rejected() {
        let cfg = Config { max_tasks: 0, enable_logging: false };
        assert!(cfg.process().is_err());
    }
}

/// Module extracted from the SigmaOS-system monolith.
pub mod task_manager;
