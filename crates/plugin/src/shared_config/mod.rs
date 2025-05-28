//! APIs and data structures for receiving runtime configuration from the Javy CLI.

use anyhow::Result;
use javy_plugin_api::Config;
use serde::Deserialize;
use std::io::{stdout, Write};

mod runtime_config;

use crate::runtime_config;

runtime_config! {
    #[derive(Debug, Default, Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "kebab-case")]
    pub struct SharedConfig {
        /// Whether to enable the `Javy.readSync` and `Javy.writeSync` builtins.
        javy_stream_io: Option<bool>,
        /// Whether to override the `JSON.parse` and `JSON.stringify`
        /// implementations with an alternative, more performant, SIMD based
        /// implemetation.
        simd_json_builtins: Option<bool>,
        /// Whether to enable support for the `TextEncoder` and `TextDecoder`
        /// APIs.
        text_encoding: Option<bool>,
        /// Whether to enable the event loop.
        event_loop: Option<bool>,
        /// Whether to enable timer APIs (`setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`).
        timers: Option<bool>,
        /// Whether to redirect console.log output to stderr instead of stdout.
        redirect_stdout_to_stderr: Option<bool>,
        /// Whether to wait for async operations (timers, promises) to complete before exiting.
        wait_for_completion: Option<bool>,
    }
}

// Additional fields that can't be handled by the runtime_config macro
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct SharedConfigExtended {
    #[serde(flatten)]
    pub base: SharedConfig,
    /// Maximum time to wait for async operations in milliseconds.
    pub wait_timeout_ms: Option<u64>,
}

impl SharedConfig {
    pub fn parse_from_json(config: &[u8]) -> Result<Self> {
        // First try to parse as extended config to get timeout parameter
        let extended: SharedConfigExtended = serde_json::from_slice(config)?;
        Ok(extended.base)
    }

    pub fn apply_to_config(&self, config: &mut Config) {
        if let Some(enable) = self.javy_stream_io {
            config.javy_stream_io(enable);
        }
        if let Some(enable) = self.simd_json_builtins {
            config.simd_json_builtins(enable);
        }
        if let Some(enable) = self.text_encoding {
            config.text_encoding(enable);
        }
        if let Some(enable) = self.event_loop {
            config.event_loop(enable);
        }
        if let Some(enable) = self.timers {
            config.timers(enable);
        }
        if let Some(enable) = self.redirect_stdout_to_stderr {
            config.redirect_stdout_to_stderr(enable);
        }
        if let Some(enable) = self.wait_for_completion {
            config.wait_for_completion(enable);
        }
    }
}

impl SharedConfigExtended {
    pub fn parse_extended_from_json(config: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice::<Self>(config)?)
    }
    
    pub fn apply_to_config(&self, config: &mut Config) {
        // Apply base config
        self.base.apply_to_config(config);
        
        // Apply timeout parameter
        if let Some(timeout_ms) = self.wait_timeout_ms {
            config.wait_timeout_ms(Some(timeout_ms));
        }
    }
}

#[export_name = "config_schema"]
pub fn config_schema() {
    // Get the base schema from the macro
    let mut base_schema = SharedConfig::config_schema();
    
    // Add the wait-timeout-ms parameter
    base_schema.supported_properties.push(
        crate::shared_config::runtime_config::ConfigProperty {
            name: "wait-timeout-ms".to_string(),
            doc: "Maximum time to wait for async operations in milliseconds.\n".to_string(),
        }
    );
    
    stdout()
        .write_all(
            serde_json::to_string(&base_schema)
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
    stdout().flush().unwrap();
}
