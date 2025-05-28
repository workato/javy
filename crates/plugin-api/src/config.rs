use std::ops::{Deref, DerefMut};

#[derive(Default)]
/// A configuration for the Javy plugin API.
pub struct Config {
    /// The runtime config.
    pub(crate) runtime_config: javy::Config,
    /// Whether to enable the event loop.
    pub(crate) event_loop: bool,
    /// Whether to wait for async operations (timers, promises) to complete before exiting.
    pub(crate) wait_for_completion: bool,
    /// Maximum time to wait for async operations in milliseconds. None means infinite wait.
    pub(crate) wait_timeout_ms: Option<u64>,
}

impl Config {
    /// Whether to enable the event loop.
    pub fn event_loop(&mut self, enabled: bool) -> &mut Self {
        self.event_loop = enabled;
        self
    }

    /// Whether to enable timer APIs (`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`).
    pub fn timers(&mut self, enabled: bool) -> &mut Self {
        self.runtime_config.timers(enabled);
        self
    }

    /// Whether to wait for async operations (timers, promises) to complete before exiting.
    /// This enables a proper event loop that will wait for delayed timers and promises.
    /// Requires event_loop to be enabled.
    pub fn wait_for_completion(&mut self, enabled: bool) -> &mut Self {
        self.wait_for_completion = enabled;
        self
    }

    /// Set the maximum time to wait for async operations in milliseconds.
    /// None means infinite wait (default). Only applies when wait_for_completion is enabled.
    pub fn wait_timeout_ms(&mut self, timeout_ms: Option<u64>) -> &mut Self {
        self.wait_timeout_ms = timeout_ms;
        self
    }
}

impl Deref for Config {
    type Target = javy::Config;

    fn deref(&self) -> &Self::Target {
        &self.runtime_config
    }
}

impl DerefMut for Config {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime_config
    }
}
