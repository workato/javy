use crate::{
    js_config::{ConfigSchema, JsConfig},
    option::OptionMeta,
    option_group, CliPlugin, WitOptions,
};
use anyhow::{anyhow, bail, Result};
use clap::{
    builder::{StringValueParser, TypedValueParser, ValueParserFactory},
    error::ErrorKind,
    CommandFactory, Parser, Subcommand,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::option::{
    fmt_help, GroupDescriptor, GroupOption, GroupOptionBuilder, GroupOptionParser, OptionValue,
};

#[derive(Debug, Parser)]
#[command(
    name = "javy",
    version,
    about = "JavaScript to WebAssembly toolchain",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compiles JavaScript to WebAssembly.
    ///
    /// NOTICE:
    ///
    /// This command is deprecated and will be removed.
    ///
    /// Refer to https://github.com/bytecodealliance/javy/issues/702 for
    /// details.
    ///
    /// Use the `build` command instead.
    #[command(arg_required_else_help = true)]
    Compile(CompileCommandOpts),
    /// Generates WebAssembly from a JavaScript source.
    #[command(arg_required_else_help = true)]
    Build(BuildCommandOpts),
    /// Emits the plugin binary that is required to run dynamically
    /// linked WebAssembly modules.
    EmitPlugin(EmitPluginCommandOpts),
    /// Initializes a plugin binary.
    #[command(arg_required_else_help = true)]
    InitPlugin(InitPluginCommandOpts),
}

#[derive(Debug, Parser)]
pub struct CompileCommandOpts {
    #[arg(value_name = "INPUT", required = true)]
    /// Path of the JavaScript input file.
    pub input: PathBuf,

    #[arg(short, default_value = "index.wasm")]
    /// Desired path of the WebAssembly output file.
    pub output: PathBuf,

    #[arg(short)]
    /// Creates a smaller module that requires a dynamically linked QuickJS
    /// plugin Wasm module to execute (see `emit-plugin` command).
    pub dynamic: bool,

    #[structopt(long)]
    /// Optional path to WIT file describing exported functions.
    /// Only supports function exports with no arguments and no return values.
    pub wit: Option<PathBuf>,

    #[arg(short = 'n')]
    /// Optional WIT world name for WIT file. Must be specified if WIT is file path is
    /// specified.
    pub wit_world: Option<String>,

    #[arg(long = "no-source-compression")]
    /// Disable source code compression, which reduces compile time at the expense of generating larger WebAssembly files.
    pub no_source_compression: bool,
}

const RUNTIME_CONFIG_ARG_SHORT: char = 'J';
const RUNTIME_CONFIG_ARG_LONG: &str = "javascript";

#[derive(Debug, Parser)]
pub struct BuildCommandOpts {
    #[arg(value_name = "INPUT")]
    /// Path of the JavaScript input file.
    pub input: Option<PathBuf>,

    #[arg(short, default_value = "index.wasm")]
    /// Desired path of the WebAssembly output file.
    pub output: PathBuf,

    #[arg(short = 'C', long = "codegen")]
    /// Code generation options.
    /// Use `-C help` for more details.
    pub codegen: Vec<GroupOption<CodegenOption>>,

    #[arg(short = RUNTIME_CONFIG_ARG_SHORT, long = RUNTIME_CONFIG_ARG_LONG)]
    /// JavaScript runtime options.
    /// Use `-J help` for more details.
    pub js: Vec<JsGroupValue>,
}

#[derive(Debug, Parser)]
pub struct EmitPluginCommandOpts {
    #[structopt(short, long)]
    /// Output path for the plugin binary (default is stdout).
    pub out: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub struct InitPluginCommandOpts {
    #[arg(value_name = "PLUGIN", required = true)]
    /// Path to the plugin to initialize.
    pub plugin: PathBuf,
    #[arg(short, long = "out")]
    /// Output path for the initialized plugin binary (default is stdout).
    pub out: Option<PathBuf>,
}

impl<T> ValueParserFactory for GroupOption<T>
where
    T: GroupDescriptor,
{
    type Parser = GroupOptionParser<T>;

    fn value_parser() -> Self::Parser {
        GroupOptionParser(std::marker::PhantomData)
    }
}

impl<T> TypedValueParser for GroupOptionParser<T>
where
    T: GroupDescriptor + GroupOptionBuilder,
{
    type Value = GroupOption<T>;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let val = StringValueParser::new().parse_ref(cmd, arg, value)?;
        let arg = arg.expect("argument to be defined");
        let short = arg.get_short().expect("short version to be defined");
        let long = arg.get_long().expect("long version to be defined");

        if val == "help" {
            fmt_help(long, &short.to_string(), &T::options());
            std::process::exit(0);
        }

        let mut opts = vec![];

        for val in val.split(',') {
            opts.push(T::parse(val).map_err(|e| {
                clap::Error::raw(clap::error::ErrorKind::InvalidValue, format!("{}", e))
            })?)
        }

        Ok(GroupOption(opts))
    }
}

/// Code generation option group.
/// This group gets configured from the [`CodegenOption`] enum.
//
// NB: The documentation for each field is ommitted given that it's similar to
// the enum used to configured the group.
#[derive(Clone, Debug, PartialEq)]
pub struct CodegenOptionGroup {
    pub dynamic: bool,
    pub wit: WitOptions,
    pub source_compression: bool,
    pub plugin: Option<PathBuf>,
}

impl Default for CodegenOptionGroup {
    fn default() -> Self {
        Self {
            dynamic: false,
            wit: WitOptions::default(),
            source_compression: true,
            plugin: None,
        }
    }
}

option_group! {
    #[derive(Clone, Debug)]
    pub enum CodegenOption {
        /// Creates a smaller module that requires a dynamically linked QuickJS
        /// plugin Wasm module to execute (see `emit-plugin` command).
        Dynamic(bool),
        /// Optional path to WIT file describing exported functions. Only
        /// supports function exports with no arguments and no return values.
        Wit(PathBuf),
        /// Optional WIT world name for WIT file. Must be specified if WIT is
        /// file path is specified.
        WitWorld(String),
        /// Enable source code compression, which generates smaller WebAssembly
        /// files at the cost of increased compile time.
        SourceCompression(bool),
        /// Optional path to Javy plugin Wasm module. Required for dynamically
        /// linked modules. JavaScript config options are also not supported when
        /// using this parameter.
        Plugin(PathBuf),
    }
}

impl TryFrom<Vec<GroupOption<CodegenOption>>> for CodegenOptionGroup {
    type Error = anyhow::Error;

    fn try_from(value: Vec<GroupOption<CodegenOption>>) -> Result<Self, Self::Error> {
        let mut options = Self::default();
        let mut wit = None;
        let mut wit_world = None;

        let mut dynamic_specified = false;
        let mut wit_specified = false;
        let mut wit_world_specified = false;
        let mut source_compression_specified = false;
        let mut plugin_specified = false;

        for option in value.iter().flat_map(|i| i.0.iter()) {
            match option {
                CodegenOption::Dynamic(enabled) => {
                    if dynamic_specified {
                        bail!("dynamic can only be specified once");
                    }
                    options.dynamic = *enabled;
                    dynamic_specified = true;
                }
                CodegenOption::Wit(path) => {
                    if wit_specified {
                        bail!("wit can only be specified once");
                    }
                    wit = Some(path);
                    wit_specified = true;
                }
                CodegenOption::WitWorld(world) => {
                    if wit_world_specified {
                        bail!("wit-world can only be specified once");
                    }
                    wit_world = Some(world);
                    wit_world_specified = true;
                }
                CodegenOption::SourceCompression(enabled) => {
                    if source_compression_specified {
                        bail!("source-compression can only be specified once");
                    }
                    options.source_compression = *enabled;
                    source_compression_specified = true;
                }
                CodegenOption::Plugin(path) => {
                    if plugin_specified {
                        bail!("plugin can only be specified once");
                    }
                    options.plugin = Some(path.clone());
                    plugin_specified = true;
                }
            }
        }

        options.wit = WitOptions::from_tuple((wit.cloned(), wit_world.cloned()))?;

        // We never want to assume the import namespace to use for a
        // dynamically linked module. If we do assume the import namespace, any
        // change to that assumed import namespace can result in new
        // dynamically linked modules not working on existing execution
        // environments because there will be unmet import errors when trying
        // to instantiate those modules. Since we can't assume the import
        // namespace, we must require a plugin so we can derive the import
        // namespace from the plugin.
        if options.dynamic && options.plugin.is_none() {
            bail!("Must specify plugin when using dynamic linking");
        }

        Ok(options)
    }
}

/// A runtime config group value.
#[derive(Debug, Clone)]
pub enum JsGroupValue {
    Option(JsGroupOption),
    Help,
}

/// The value type for a runtime config option.
#[derive(Debug, Clone)]
pub enum JsOptionValue {
    Boolean(bool),
    Number(u64),
}

/// A runtime config group option.
#[derive(Debug, Clone)]
pub struct JsGroupOption {
    /// The property name used for the option.
    name: String,
    /// The value of the config option.
    value: JsOptionValue,
}

#[derive(Debug, Clone)]
pub struct JsGroupOptionParser;

impl ValueParserFactory for JsGroupValue {
    type Parser = JsGroupOptionParser;

    fn value_parser() -> Self::Parser {
        JsGroupOptionParser
    }
}

impl TypedValueParser for JsGroupOptionParser {
    type Value = JsGroupValue;

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &std::ffi::OsStr,
    ) -> std::result::Result<Self::Value, clap::Error> {
        let val = StringValueParser::new().parse_ref(cmd, arg, value)?;

        if val == "help" {
            // We need to display help immediately and exit, but we don't have access to
            // the plugin here to get the supported properties. We'll need to handle this
            // differently by still returning Help and letting the caller handle it.
            return Ok(JsGroupValue::Help);
        }

        let mut splits = val.splitn(2, '=');
        let key = splits.next().unwrap();
        let value_str = splits.next();
        
        let option_value = match (key, value_str) {
            // Special handling for wait-timeout-ms which expects a number
            ("wait-timeout-ms", Some(num_str)) => {
                match num_str.parse::<u64>() {
                    Ok(num) => JsOptionValue::Number(num),
                    Err(_) => return Err(clap::Error::new(clap::error::ErrorKind::InvalidValue)),
                }
            }
            ("wait-timeout-ms", None) => {
                return Err(clap::Error::new(clap::error::ErrorKind::InvalidValue));
            }
            // All other options are boolean
            (_, Some("y")) => JsOptionValue::Boolean(true),
            (_, Some("n")) => JsOptionValue::Boolean(false),
            (_, None) => JsOptionValue::Boolean(true),
            (_, Some(_)) => return Err(clap::Error::new(clap::error::ErrorKind::InvalidValue)),
        };
        
        Ok(JsGroupValue::Option(JsGroupOption {
            name: key.to_string(),
            value: option_value,
        }))
    }
}

impl JsConfig {
    /// Build a JS runtime config from valid runtime config values.
    pub(super) fn from_group_values(
        cli_plugin: &CliPlugin,
        group_values: Vec<JsGroupValue>,
    ) -> Result<JsConfig> {
        // Always attempt to fetch the supported properties from a plugin.
        let supported_properties = ConfigSchema::from_cli_plugin(cli_plugin)?
            .map_or(Vec::new(), |schema| schema.supported_properties);

        let mut supported_names = HashSet::new();
        for property in &supported_properties {
            supported_names.insert(property.name.as_str());
        }

        let mut config = HashMap::new();
        for value in group_values {
            match value {
                JsGroupValue::Help => {
                    fmt_help(
                        RUNTIME_CONFIG_ARG_LONG,
                        &RUNTIME_CONFIG_ARG_SHORT.to_string(),
                        &supported_properties
                            .into_iter()
                            .map(|prop| OptionMeta {
                                name: prop.name.clone(),
                                help: if prop.name == "wait-timeout-ms" {
                                    "=<milliseconds>".to_string()
                                } else {
                                    "[=y|n]".to_string()
                                },
                                doc: prop.doc,
                            })
                            .collect::<Vec<_>>(),
                    );
                    std::process::exit(0);
                }
                JsGroupValue::Option(JsGroupOption { name, value }) => {
                    if supported_names.contains(name.as_str()) {
                        if config.contains_key(&name) {
                            bail!("{name} can only be specified once");
                        }
                        config.insert(name, value);
                    } else {
                        Cli::command()
                            .error(
                                ErrorKind::InvalidValue,
                                format!(
                                    "Property {name} is not supported for runtime configuration",
                                ),
                            )
                            .exit();
                    }
                }
            }
        }
        
        // Validate configuration dependencies
        if let (Some(wait_completion), event_loop) = (config.get("wait-for-completion"), config.get("event-loop")) {
            if let JsOptionValue::Boolean(true) = wait_completion {
                if !matches!(event_loop, Some(JsOptionValue::Boolean(true))) {
                    bail!("wait-for-completion requires event-loop to be enabled. Use: -J event-loop=y -J wait-for-completion=y");
                }
            }
        }
        
        Ok(JsConfig::from_hash(config))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        commands::{JsGroupOption, JsGroupValue, JsOptionValue},
        js_config::JsConfig,
        plugin::PLUGIN_MODULE,
        CliPlugin, Plugin, PluginKind,
    };

    use super::{CodegenOption, CodegenOptionGroup, GroupOption};
    use anyhow::{Error, Result};

    #[test]
    fn js_config_from_config_values() -> Result<()> {
        let plugin = CliPlugin::new(Plugin::new(PLUGIN_MODULE.into()), PluginKind::Default);

        let group = JsConfig::from_group_values(&plugin, vec![])?;
        assert_eq!(group.get("javy-stream-io"), None);
        assert_eq!(group.get("simd-json-builtins"), None);
        assert_eq!(group.get("text-encoding"), None);

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "javy-stream-io".to_string(),
                value: JsOptionValue::Boolean(false),
            })],
        )?;
        assert_eq!(group.get("javy-stream-io"), Some(false));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "javy-stream-io".to_string(),
                value: JsOptionValue::Boolean(true),
            })],
        )?;
        assert_eq!(group.get("javy-stream-io"), Some(true));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "simd-json-builtins".to_string(),
                value: JsOptionValue::Boolean(false),
            })],
        )?;
        assert_eq!(group.get("simd-json-builtins"), Some(false));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "simd-json-builtins".to_string(),
                value: JsOptionValue::Boolean(true),
            })],
        )?;
        assert_eq!(group.get("simd-json-builtins"), Some(true));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "text-encoding".to_string(),
                value: JsOptionValue::Boolean(false),
            })],
        )?;
        assert_eq!(group.get("text-encoding"), Some(false));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "text-encoding".to_string(),
                value: JsOptionValue::Boolean(true),
            })],
        )?;
        assert_eq!(group.get("text-encoding"), Some(true));

        let group = JsConfig::from_group_values(
            &plugin,
            vec![
                JsGroupValue::Option(JsGroupOption {
                    name: "javy-stream-io".to_string(),
                    value: JsOptionValue::Boolean(false),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "simd-json-builtins".to_string(),
                    value: JsOptionValue::Boolean(false),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "text-encoding".to_string(),
                    value: JsOptionValue::Boolean(false),
                }),
            ],
        )?;
        assert_eq!(group.get("javy-stream-io"), Some(false));
        assert_eq!(group.get("simd-json-builtins"), Some(false));
        assert_eq!(group.get("text-encoding"), Some(false));

        Ok(())
    }

    #[test]
    fn codegen_group_conversion_between_vector_of_options_and_group() -> Result<()> {
        let group: CodegenOptionGroup = vec![].try_into()?;
        assert_eq!(group, CodegenOptionGroup::default());

        let raw = vec![GroupOption(vec![
            CodegenOption::Dynamic(true),
            CodegenOption::Plugin(PathBuf::from("file.wasm")),
        ])];
        let group: CodegenOptionGroup = raw.try_into()?;
        let expected = CodegenOptionGroup {
            dynamic: true,
            plugin: Some(PathBuf::from("file.wasm")),
            ..Default::default()
        };

        assert_eq!(group, expected);

        let raw = vec![GroupOption(vec![CodegenOption::SourceCompression(false)])];
        let group: CodegenOptionGroup = raw.try_into()?;
        let expected = CodegenOptionGroup {
            source_compression: false,
            ..Default::default()
        };

        assert_eq!(group, expected);

        let raw = vec![GroupOption(vec![CodegenOption::Dynamic(true)])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "Must specify plugin when using dynamic linking"
        );

        Ok(())
    }

    #[test]
    fn codegen_option_specified_twice_should_return_error() -> Result<()> {
        let raw = vec![GroupOption(vec![
            CodegenOption::Dynamic(true),
            CodegenOption::Dynamic(false),
        ])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "dynamic can only be specified once"
        );

        let raw = vec![GroupOption(vec![
            CodegenOption::Wit(PathBuf::from("file.wit")),
            CodegenOption::Wit(PathBuf::from("file2.wit")),
        ])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "wit can only be specified once"
        );

        let raw = vec![GroupOption(vec![
            CodegenOption::WitWorld("world".to_string()),
            CodegenOption::WitWorld("world2".to_string()),
        ])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "wit-world can only be specified once"
        );

        let raw = vec![GroupOption(vec![
            CodegenOption::SourceCompression(true),
            CodegenOption::SourceCompression(false),
        ])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "source-compression can only be specified once"
        );

        let raw = vec![GroupOption(vec![
            CodegenOption::Plugin(PathBuf::from("file.wasm")),
            CodegenOption::Plugin(PathBuf::from("file2.wasm")),
        ])];
        let result: Result<CodegenOptionGroup, Error> = raw.try_into();
        assert_eq!(
            result.err().unwrap().to_string(),
            "plugin can only be specified once"
        );
        Ok(())
    }

    #[test]
    fn js_option_specified_twice_should_return_error() {
        let plugin = CliPlugin::new(Plugin::new(PLUGIN_MODULE.into()), PluginKind::Default);
        let result = JsConfig::from_group_values(
            &plugin,
            vec![
                JsGroupValue::Option(JsGroupOption {
                    name: "javy-stream-io".to_string(),
                    value: JsOptionValue::Boolean(false),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "javy-stream-io".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
            ],
        );
        assert_eq!(
            result.err().unwrap().to_string(),
            "javy-stream-io can only be specified once"
        );
    }

    #[test]
    fn wait_for_completion_requires_event_loop() {
        let plugin = CliPlugin::new(Plugin::new(PLUGIN_MODULE.into()), PluginKind::Default);
        
        // Test: wait-for-completion=y without event-loop should fail
        let result = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "wait-for-completion".to_string(),
                value: JsOptionValue::Boolean(true),
            })],
        );
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("wait-for-completion requires event-loop to be enabled"));
        
        // Test: wait-for-completion=y with event-loop=n should fail
        let result = JsConfig::from_group_values(
            &plugin,
            vec![
                JsGroupValue::Option(JsGroupOption {
                    name: "event-loop".to_string(),
                    value: JsOptionValue::Boolean(false),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "wait-for-completion".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
            ],
        );
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("wait-for-completion requires event-loop to be enabled"));
        
        // Test: wait-for-completion=y with event-loop=y should succeed
        let result = JsConfig::from_group_values(
            &plugin,
            vec![
                JsGroupValue::Option(JsGroupOption {
                    name: "event-loop".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "wait-for-completion".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
            ],
        );
        assert!(result.is_ok());
        
        // Test: wait-for-completion=n should always succeed regardless of event-loop
        let result = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "wait-for-completion".to_string(),
                value: JsOptionValue::Boolean(false),
            })],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn wait_timeout_ms_parameter_parsing() {
        let plugin = CliPlugin::new(Plugin::new(PLUGIN_MODULE.into()), PluginKind::Default);
        
        // Test: wait-timeout-ms with numeric value should succeed
        let result = JsConfig::from_group_values(
            &plugin,
            vec![
                JsGroupValue::Option(JsGroupOption {
                    name: "event-loop".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "wait-for-completion".to_string(),
                    value: JsOptionValue::Boolean(true),
                }),
                JsGroupValue::Option(JsGroupOption {
                    name: "wait-timeout-ms".to_string(),
                    value: JsOptionValue::Number(5000),
                }),
            ],
        );
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.get_number("wait-timeout-ms"), Some(5000));
        
        // Test: wait-timeout-ms with different numeric values
        let result = JsConfig::from_group_values(
            &plugin,
            vec![JsGroupValue::Option(JsGroupOption {
                name: "wait-timeout-ms".to_string(),
                value: JsOptionValue::Number(1000),
            })],
        );
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.get_number("wait-timeout-ms"), Some(1000));
    }
}
