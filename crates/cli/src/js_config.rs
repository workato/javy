use anyhow::Result;
use serde::Deserialize;
use std::{collections::HashMap, str};
use wasmtime::{AsContextMut, Engine, Linker};
use wasmtime_wasi::{pipe::MemoryOutputPipe, WasiCtxBuilder};

use crate::{CliPlugin, PluginKind, commands::JsOptionValue};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ConfigSchema {
    pub(crate) supported_properties: Vec<JsConfigProperty>,
}

impl ConfigSchema {
    pub(crate) fn from_cli_plugin(cli_plugin: &CliPlugin) -> Result<Option<ConfigSchema>> {
        match cli_plugin.kind {
            PluginKind::User => Ok(None),
            PluginKind::Default => {
                let engine = Engine::default();
                let module = wasmtime::Module::new(&engine, cli_plugin.as_plugin().as_bytes())?;
                let mut linker = Linker::new(&engine);
                wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |s| s)?;
                let stdout = MemoryOutputPipe::new(usize::MAX);
                let wasi = WasiCtxBuilder::new()
                    .inherit_stderr()
                    .stdout(stdout.clone())
                    .build_p1();
                let mut store = wasmtime::Store::new(&engine, wasi);
                let instance = linker.instantiate(store.as_context_mut(), &module)?;
                instance
                    .get_typed_func::<(), ()>(store.as_context_mut(), "config_schema")?
                    .call(store.as_context_mut(), ())?;
                drop(store);
                let config_json = stdout.try_into_inner().unwrap().to_vec();
                let config_schema = serde_json::from_slice::<ConfigSchema>(&config_json)?;
                let mut configs = Vec::with_capacity(config_schema.supported_properties.len());
                for config in config_schema.supported_properties {
                    configs.push(JsConfigProperty {
                        name: config.name,
                        doc: config.doc,
                    });
                }

                Ok(Some(Self {
                    supported_properties: configs,
                }))
            }
        }
    }
}

/// A property that is in the config schema returned by the plugin.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JsConfigProperty {
    /// The name of the property (e.g., `simd-json-builtins`).
    pub(crate) name: String,
    /// The documentation to display for the property.
    pub(crate) doc: String,
}

/// A collection of property names to their values.
#[derive(Clone, Debug, Default)]
pub(crate) struct JsConfig(HashMap<String, JsOptionValue>);

impl JsConfig {
    /// Create from a hash.
    pub(crate) fn from_hash(configs: HashMap<String, JsOptionValue>) -> Self {
        JsConfig(configs)
    }

    /// Encode as JSON.
    pub(crate) fn to_json(&self) -> Result<Vec<u8>> {
        // Convert to a JSON-serializable format
        let mut json_map = serde_json::Map::new();
        for (key, value) in &self.0 {
            match value {
                JsOptionValue::Boolean(b) => {
                    json_map.insert(key.clone(), serde_json::Value::Bool(*b));
                }
                JsOptionValue::Number(n) => {
                    json_map.insert(key.clone(), serde_json::Value::Number((*n).into()));
                }
            }
        }
        Ok(serde_json::to_vec(&json_map)?)
    }

    #[cfg(test)]
    /// Retrieve a boolean value for a property name.
    pub(crate) fn get(&self, name: &str) -> Option<bool> {
        match self.0.get(name) {
            Some(JsOptionValue::Boolean(b)) => Some(*b),
            _ => None,
        }
    }
    
    #[cfg(test)]
    /// Retrieve a numeric value for a property name.
    pub(crate) fn get_number(&self, name: &str) -> Option<u64> {
        match self.0.get(name) {
            Some(JsOptionValue::Number(n)) => Some(*n),
            _ => None,
        }
    }
}
