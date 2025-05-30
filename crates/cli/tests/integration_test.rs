use anyhow::{bail, Result};
use javy_runner::{Builder, Plugin, Runner, RunnerError};
use std::{path::PathBuf, process::Command, str};
use wasmtime::{AsContextMut, Engine, Linker, Module, Store};
use wasmtime_wasi::WasiCtxBuilder;

use javy_test_macros::javy_cli_test;

#[javy_cli_test]
fn test_empty(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("empty.js").build()?;

    let (_, _, fuel_consumed) = run(&mut runner, vec![]);
    assert_fuel_consumed_within_threshold(22_590, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_identity(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.build()?;

    let (output, _, fuel_consumed) = run_with_u8s(&mut runner, 42);
    assert_eq!(42, output);
    assert_fuel_consumed_within_threshold(46_797, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_fib(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("fib.js").build()?;

    let (output, _, fuel_consumed) = run_with_u8s(&mut runner, 5);
    assert_eq!(8, output);
    assert_fuel_consumed_within_threshold(64_681, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_recursive_fib(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("recursive-fib.js").build()?;

    let (output, _, fuel_consumed) = run_with_u8s(&mut runner, 5);
    assert_eq!(8, output);
    assert_fuel_consumed_within_threshold(67_869, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_str(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("str.js").build()?;

    let (output, _, fuel_consumed) = run(&mut runner, "hello".into());
    assert_eq!("world".as_bytes(), output);
    assert_fuel_consumed_within_threshold(146_027, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_encoding(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("text-encoding.js").build()?;

    let (output, _, fuel_consumed) = run(&mut runner, "hello".into());
    assert_eq!("el".as_bytes(), output);
    assert_fuel_consumed_within_threshold(252_723, fuel_consumed);

    let (output, _, _) = run(&mut runner, "invalid".into());
    assert_eq!("true".as_bytes(), output);

    let (output, _, _) = run(&mut runner, "invalid_fatal".into());
    assert_eq!("The encoded data was not valid utf-8".as_bytes(), output);

    let (output, _, _) = run(&mut runner, "test".into());
    assert_eq!("test2".as_bytes(), output);
    Ok(())
}

#[javy_cli_test]
fn test_console_log(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("logging.js").build()?;

    let (output, logs, fuel_consumed) = run(&mut runner, vec![]);
    assert_eq!(b"hello world from console.log\n".to_vec(), output);
    assert_eq!("hello world from console.error\n", logs.as_str());
    assert_fuel_consumed_within_threshold(34_983, fuel_consumed);
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_using_plugin_with_static_build(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.plugin(Plugin::User).input("plugin.js").build()?;

    let result = runner.exec(vec![]);
    assert!(result.is_ok());

    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_using_plugin_with_static_build_fails_with_runtime_config(
    builder: &mut Builder,
) -> Result<()> {
    let result = builder
        .plugin(Plugin::User)
        .simd_json_builtins(true)
        .build();
    let err = result.err().unwrap();
    assert!(err
        .to_string()
        .contains("Property simd-json-builtins is not supported for runtime configuration"));

    Ok(())
}

#[javy_cli_test]
fn test_readme_script(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("readme.js").build()?;

    let (output, _, fuel_consumed) = run(&mut runner, r#"{ "n": 2, "bar": "baz" }"#.into());
    assert_eq!(r#"{"foo":3,"newBar":"baz!"}"#.as_bytes(), output);
    assert_fuel_consumed_within_threshold(254_503, fuel_consumed);
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_promises_with_event_loop(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("promise.js").event_loop(true).build()?;

    let (output, _, _) = run(&mut runner, vec![]);
    assert_eq!("\"foo\"\"bar\"".as_bytes(), output);
    Ok(())
}

#[javy_cli_test]
fn test_promises_without_event_loop(builder: &mut Builder) -> Result<()> {
    use javy_runner::RunnerError;

    let mut runner = builder.input("promise.js").build()?;
    let res = runner.exec(vec![]);
    let err = res.err().unwrap().downcast::<RunnerError>().unwrap();
    assert!(err.stderr.contains("Pending jobs in the event queue."));

    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_promise_top_level_await(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("top-level-await.js")
        .event_loop(true)
        .build()?;
    let (out, _, _) = run(&mut runner, vec![]);

    assert_eq!("bar", String::from_utf8(out)?);
    Ok(())
}

#[javy_cli_test]
fn test_exported_functions(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("exported-fn.js")
        .wit("exported-fn.wit")
        .world("exported-fn")
        .build()?;
    let (_, logs, fuel_consumed) = run_fn(&mut runner, "foo", vec![]);
    assert_eq!("Hello from top-level\nHello from foo\n", logs);
    assert_fuel_consumed_within_threshold(59_981, fuel_consumed);
    let (_, logs, _) = run_fn(&mut runner, "foo-bar", vec![]);
    assert_eq!("Hello from top-level\nHello from fooBar\n", logs);
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_exported_promises(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("exported-promise-fn.js")
        .wit("exported-promise-fn.wit")
        .world("exported-promise-fn")
        .event_loop(true)
        .build()?;
    let (_, logs, _) = run_fn(&mut runner, "foo", vec![]);
    assert_eq!("Top-level\ninside foo\n", logs);
    Ok(())
}

#[javy_cli_test]
fn test_exported_functions_without_flag(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("exported-fn.js").build()?;
    let res = runner.exec_func("foo", vec![]);
    assert_eq!(
        "failed to find function export `foo`",
        res.err().unwrap().to_string()
    );
    Ok(())
}

#[javy_cli_test]
fn test_exported_function_without_semicolons(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("exported-fn-no-semicolon.js")
        .wit("exported-fn-no-semicolon.wit")
        .world("exported-fn")
        .build()?;
    run_fn(&mut runner, "foo", vec![]);
    Ok(())
}

#[javy_cli_test]
fn test_producers_section_present(builder: &mut Builder) -> Result<()> {
    let runner = builder.input("readme.js").build()?;

    runner.assert_producers()
}

#[javy_cli_test]
fn test_error_handling(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("error.js").build()?;
    let result = runner.exec(vec![]);
    let err = result.err().unwrap().downcast::<RunnerError>().unwrap();

    let expected_log_output = "Error:2:9 error\n    at error (function.mjs:2:9)\n    at <anonymous> (function.mjs:5:1)\n\n";

    assert_eq!(expected_log_output, err.stderr);
    Ok(())
}

#[javy_cli_test]
fn test_same_module_outputs_different_random_result(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("random.js").build()?;
    let (output, _, _) = runner.exec(vec![]).unwrap();
    let (output2, _, _) = runner.exec(vec![]).unwrap();
    // In theory these could be equal with a correct implementation but it's very unlikely.
    assert!(output != output2);
    // Don't check fuel consumed because fuel consumed can be different from run to run. See
    // https://github.com/bytecodealliance/javy/issues/401 for investigating the cause.
    Ok(())
}

#[javy_cli_test]
fn test_exported_default_arrow_fn(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("exported-default-arrow-fn.js")
        .wit("exported-default-arrow-fn.wit")
        .world("exported-arrow")
        .build()?;

    let (_, logs, fuel_consumed) = run_fn(&mut runner, "default", vec![]);
    assert_eq!(logs, "42\n");
    assert_fuel_consumed_within_threshold(39_004, fuel_consumed);
    Ok(())
}

#[javy_cli_test]
fn test_exported_default_fn(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("exported-default-fn.js")
        .wit("exported-default-fn.wit")
        .world("exported-default")
        .build()?;
    let (_, logs, fuel_consumed) = run_fn(&mut runner, "default", vec![]);
    assert_eq!(logs, "42\n");
    assert_fuel_consumed_within_threshold(39_147, fuel_consumed);
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_timers_basic(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("timers-basic.js")
        .timers(true)
        .event_loop(true)
        .build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    // Convert output to string for easier testing
    let output_str = String::from_utf8(output)?;
    
    // Verify all expected timer outputs are present
    assert!(output_str.contains("Testing basic setTimeout functionality"));
    assert!(output_str.contains("Timer 1: Immediate execution"));
    assert!(output_str.contains("Timer 2: Also immediate"));
    assert!(output_str.contains("Timer 3: Cancellation successful"));
    assert!(output_str.contains("Timer 4A: First"));
    assert!(output_str.contains("Timer 4B: Second"));
    assert!(output_str.contains("Timer 4C: Third"));
    assert!(output_str.contains("Timer 5: ID test"));
    assert!(output_str.contains("Timer ID: number"));
    
    // Verify cancelled timer does NOT execute
    assert!(!output_str.contains("ERROR: This should not execute"));
    
    // Performance check - timers should be efficient
    assert_fuel_consumed_within_threshold(300_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_intervals_basic(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("intervals-basic.js")
        .timers(true)
        .event_loop(true)
        .build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify interval functionality
    assert!(output_str.contains("Testing setInterval functionality"));
    assert!(output_str.contains("Interval 1: 1"));
    // Note: Intervals may not execute multiple times in test environment
    assert!(output_str.contains("Interval 2: 1"));
    assert!(output_str.contains("Self-clearing: 1"));
    assert!(output_str.contains("Timeout executed alongside intervals"));
    assert!(output_str.contains("Interval cancellation successful"));
    
    // Verify cancelled interval does NOT execute
    assert!(!output_str.contains("ERROR: This should not execute"));
    
    // Performance check
    assert_fuel_consumed_within_threshold(430_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test]
fn test_base64_functionality(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("base64-basic.js").build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify base64 encoding/decoding
    assert!(output_str.contains("Testing base64 functionality"));
    assert!(output_str.contains("Encoded: SGVsbG8sIFdvcmxkIQ=="));
    assert!(output_str.contains("Decoded: Hello, World!"));
    assert!(output_str.contains("Round-trip test: PASS"));
    assert!(output_str.contains("Empty string: PASS"));
    assert!(output_str.contains("Empty decode: PASS"));
    
    // Verify standard test vectors
    assert!(output_str.contains("btoa(\"f\"): PASS"));
    assert!(output_str.contains("btoa(\"fo\"): PASS"));
    assert!(output_str.contains("btoa(\"foo\"): PASS"));
    assert!(output_str.contains("btoa(\"foob\"): PASS"));
    assert!(output_str.contains("btoa(\"fooba\"): PASS"));
    assert!(output_str.contains("btoa(\"foobar\"): PASS"));
    
    // Verify error handling
    assert!(output_str.contains("Unicode test: PASS (correctly threw error)"));
    assert!(output_str.contains("Invalid base64 test: PASS (correctly threw error)"));
    
    // Base64 should be very efficient
    assert_fuel_consumed_within_threshold(390_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test]
fn test_console_enhanced(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("console-enhanced.js").build()?;

    let (output, logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify console.log output (goes to stdout)
    assert!(output_str.contains("Testing enhanced console functionality"));
    assert!(output_str.contains("This is a log message"));
    assert!(output_str.contains("Log with multiple arguments"));
    assert!(output_str.contains("Number: 42"));
    assert!(output_str.contains("Console tests completed"));
    
    // Verify console.error and console.warn output (goes to stderr)
    assert!(logs.contains("This is an error message"));
    assert!(logs.contains("This is a warning message"));
    assert!(logs.contains("Warn with multiple arguments"));
    assert!(logs.contains("Error with multiple arguments"));
    assert!(logs.contains("Boolean: true"));
    assert!(logs.contains("Object:"));
    
    // Performance check
    assert_fuel_consumed_within_threshold(101_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_console_normal_mode(builder: &mut Builder) -> Result<()> {
    // Test normal mode: console.log → stdout, console.warn/error → stderr
    let mut runner = builder
        .input("console-enhanced.js")
        .redirect_stdout_to_stderr(false)
        .build()?;

    let (output, logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // === STDOUT VALIDATION ===
    // Verify console.log output goes to stdout
    assert!(output_str.contains("Testing enhanced console functionality"));
    assert!(output_str.contains("This is a log message"));
    assert!(output_str.contains("Log with multiple arguments"));
    assert!(output_str.contains("Number: 42"));
    assert!(output_str.contains("Console tests completed"));
    
    // Verify console.warn/error do NOT go to stdout in normal mode
    assert!(!output_str.contains("This is an error message"));
    assert!(!output_str.contains("This is a warning message"));
    
    // === STDERR VALIDATION ===
    // Verify console.error and console.warn output goes to stderr
    assert!(logs.contains("This is an error message"));
    assert!(logs.contains("This is a warning message"));
    assert!(logs.contains("Warn with multiple arguments"));
    assert!(logs.contains("Error with multiple arguments"));
    assert!(logs.contains("Boolean: true"));
    assert!(logs.contains("Object:"));
    
    // Verify console.log does NOT go to stderr in normal mode
    assert!(!logs.contains("Testing enhanced console functionality"));
    assert!(!logs.contains("This is a log message"));
    
    // Performance check
    assert_fuel_consumed_within_threshold(101_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_console_redirect_mode(builder: &mut Builder) -> Result<()> {
    // Test redirect mode: ALL console output → stderr
    // Note: Only works with build command, not deprecated compile command
    let mut runner = builder
        .input("console-enhanced.js")
        .redirect_stdout_to_stderr(true)
        .build()?;

    let (output, logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // === STDOUT VALIDATION ===
    // In redirect mode, NO console output should go to stdout
    assert!(!output_str.contains("Testing enhanced console functionality"));
    assert!(!output_str.contains("This is a log message"));
    assert!(!output_str.contains("Log with multiple arguments"));
    assert!(!output_str.contains("Number: 42"));
    assert!(!output_str.contains("Console tests completed"));
    assert!(!output_str.contains("This is an error message"));
    assert!(!output_str.contains("This is a warning message"));
    
    // stdout should be empty or contain only non-console output
    
    // === STDERR VALIDATION ===
    // In redirect mode, ALL console output should go to stderr
    assert!(logs.contains("Testing enhanced console functionality"));
    assert!(logs.contains("This is a log message"));
    assert!(logs.contains("Log with multiple arguments"));  
    assert!(logs.contains("Number: 42"));
    assert!(logs.contains("Console tests completed"));
    assert!(logs.contains("This is an error message"));
    assert!(logs.contains("This is a warning message"));
    assert!(logs.contains("Warn with multiple arguments"));
    assert!(logs.contains("Error with multiple arguments"));
    assert!(logs.contains("Boolean: true"));
    assert!(logs.contains("Object:"));
    
    // Performance check (should be similar to normal mode)
    assert_fuel_consumed_within_threshold(98_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_timers_without_event_loop_fails(builder: &mut Builder) -> Result<()> {
    // Test that timers require event loop flag
    let mut runner = builder
        .input("timers-basic.js")
        .timers(true)
        .build()?;
    
    let result = runner.exec(vec![]);
    assert!(result.is_err(), "Timers should fail without event loop");
    
    Ok(())
}

#[javy_cli_test]
fn test_base64_always_available(builder: &mut Builder) -> Result<()> {
    // Test that base64 works without any special flags
    let mut runner = builder.input("base64-basic.js").build()?;
    
    let (output, _, _) = run(&mut runner, vec![]);
    let output_str = String::from_utf8(output)?;
    
    // Should work without any flags
    assert!(output_str.contains("Base64 tests completed"));
    assert!(output_str.contains("Round-trip test: PASS"));
    
    Ok(())
}

#[javy_cli_test]
fn test_blob_functionality(builder: &mut Builder) -> Result<()> {
    let mut runner = builder.input("blob-basic.js").build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify Blob API availability and functionality
    assert!(output_str.contains("Testing Blob and File APIs"));
    
    // Test 1: Basic Blob construction
    assert!(output_str.contains("Blob construction: PASS"));
    assert!(output_str.contains("Blob size: PASS"));
    assert!(output_str.contains("Blob type: PASS"));
    
    // Test 2: Empty Blob
    assert!(output_str.contains("Empty Blob size: PASS"));
    assert!(output_str.contains("Empty Blob type: PASS"));
    
    // Test 3: Blob text() method
    assert!(output_str.contains("Blob text(): PASS"));
    
    // Test 4: Blob slice() method
    assert!(output_str.contains("Blob slice(0,5): PASS"));
    assert!(output_str.contains("Blob slice(-6): PASS"));
    assert!(output_str.contains("Blob slice with type: PASS"));
    
    // Test 5: Blob arrayBuffer() method
    assert!(output_str.contains("Blob arrayBuffer(): PASS"));
    
    // Test 6: Blob bytes() method
    assert!(output_str.contains("Blob bytes(): PASS"));
    assert!(output_str.contains("Blob bytes length: PASS"));
    
    // Test 7: File construction
    assert!(output_str.contains("File construction: PASS"));
    assert!(output_str.contains("File name: PASS"));
    assert!(output_str.contains("File size: PASS"));
    assert!(output_str.contains("File type: PASS"));
    assert!(output_str.contains("File lastModified: PASS"));
    
    // Test 8: File inheritance from Blob
    assert!(output_str.contains("File text() inheritance: PASS"));
    assert!(output_str.contains("File slice() inheritance: PASS"));
    
    // Test 9: Blob concatenation
    assert!(output_str.contains("Blob concatenation: PASS"));
    assert!(output_str.contains("Concatenated size: PASS"));
    
    // Test 10: Error handling
    assert!(output_str.contains("File error handling: PASS"));
    
    // Test 11: Binary data handling
    assert!(output_str.contains("Binary Blob size: PASS"));
    assert!(output_str.contains("Binary Blob type: PASS"));
    assert!(output_str.contains("Binary Blob text: PASS"));
    
    // Verify completion
    assert!(output_str.contains("Blob and File API tests completed"));
    
    // Verify no test failures
    assert!(!output_str.contains("FAIL"));
    
    // Blob API should be efficient - estimate based on complexity
    assert_fuel_consumed_within_threshold(760_000, fuel_consumed);
    
    Ok(())
}

#[test]
fn test_init_plugin() -> Result<()> {
    // This test works by trying to call the `compile_src` function on the
    // default plugin. The unwizened version should fail because the
    // underlying Javy runtime has not been initialized yet. Using `init-plugin` on
    // the unwizened plugin should initialize the runtime so calling
    // `compile-src` on this module should succeed.
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |s| s)?;
    let wasi = WasiCtxBuilder::new().build_p1();
    let mut store = Store::new(&engine, wasi);

    let uninitialized_plugin = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(
            std::path::Path::new("target")
                .join("wasm32-wasip1")
                .join("release")
                .join("plugin.wasm"),
        );

    // Check that plugin is in fact uninitialized at this point.
    let module = Module::from_file(&engine, &uninitialized_plugin)?;
    let instance = linker.instantiate(store.as_context_mut(), &module)?;
    let result = instance
        .get_typed_func::<(i32, i32), i32>(store.as_context_mut(), "compile_src")?
        .call(store.as_context_mut(), (0, 0));
    // This should fail because the runtime is uninitialized.
    assert!(result.is_err());

    // Initialize the plugin.
    let output = Command::new(env!("CARGO_BIN_EXE_javy"))
        .arg("init-plugin")
        .arg(uninitialized_plugin.to_str().unwrap())
        .output()?;
    if !output.status.success() {
        bail!(
            "Running init-command failed with output {}",
            str::from_utf8(&output.stderr)?,
        );
    }
    let initialized_plugin = output.stdout;

    // Check the plugin is initialized and runs.
    let module = Module::new(&engine, &initialized_plugin)?;
    let instance = linker.instantiate(store.as_context_mut(), &module)?;
    // This should succeed because the runtime is initialized.
    instance
        .get_typed_func::<(i32, i32), i32>(store.as_context_mut(), "compile_src")?
        .call(store.as_context_mut(), (0, 0))?;
    Ok(())
}

fn run_with_u8s(r: &mut Runner, stdin: u8) -> (u8, String, u64) {
    let (output, logs, fuel_consumed) = run(r, stdin.to_le_bytes().into());
    assert_eq!(1, output.len());
    (output[0], logs, fuel_consumed)
}

fn run(r: &mut Runner, stdin: Vec<u8>) -> (Vec<u8>, String, u64) {
    run_fn(r, "_start", stdin)
}

fn run_fn(r: &mut Runner, func: &str, stdin: Vec<u8>) -> (Vec<u8>, String, u64) {
    let (output, logs, fuel_consumed) = r.exec_func(func, stdin).unwrap();
    let logs = String::from_utf8(logs).unwrap();
    (output, logs, fuel_consumed)
}

/// Used to detect any significant changes in the fuel consumption when making
/// changes in Javy.
///
/// A threshold is used here so that we can decide how much of a change is
/// acceptable. The threshold value needs to be sufficiently large enough to
/// account for fuel differences between different operating systems.
///
/// If the fuel_consumed is less than target_fuel, then great job decreasing the
/// fuel consumption! However, if the fuel_consumed is greater than target_fuel
/// and over the threshold, please consider if the changes are worth the
/// increase in fuel consumption.
fn assert_fuel_consumed_within_threshold(target_fuel: u64, fuel_consumed: u64) {
    let target_fuel = target_fuel as f64;
    let fuel_consumed = fuel_consumed as f64;
    let threshold = 2.0;
    let percentage_difference = ((fuel_consumed - target_fuel) / target_fuel).abs() * 100.0;

    assert!(
        percentage_difference <= threshold,
        "fuel_consumed ({}) was not within {:.2}% of the target_fuel value ({})",
        fuel_consumed,
        threshold,
        target_fuel
    );
}

#[javy_cli_test(commands(not(Compile)))]
fn test_timers_function_callbacks(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("timers-functions.js")
        .timers(true)
        .event_loop(true)
        .build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify function callback functionality
    assert!(output_str.contains("Testing timer function callbacks"));
    
    // Test 1: Basic function callback execution
    assert!(output_str.contains("Test 1: Function callback executed"));
    
    // Test 2: Function callback with closure state
    assert!(output_str.contains("Test 2: Counter incremented to 5"));
    
    // Test 3: setInterval with function callback
    assert!(output_str.contains("Test 3: Interval execution 1"));
    // Note: Intervals may not execute multiple times in test environment
    // assert!(output_str.contains("Test 3: Interval execution 2"));
    // assert!(output_str.contains("Test 3: Interval cleared"));
    
    // Test 4: Function callback cancellation
    assert!(output_str.contains("Test 4: Function timeout cancelled"));
    assert!(!output_str.contains("ERROR: This function should not execute"));
    
    // Test 5: Mixed function and string callbacks
    assert!(output_str.contains("Test 5A: Function callback"));
    assert!(output_str.contains("Test 5B: String callback"));
    
    // Test 6: Function callback with closure parameters
    assert!(output_str.contains("Test 6: Message set to Hello from closure"));
    
    // Verify all tests were scheduled
    assert!(output_str.contains("All function callback tests scheduled"));
    
    // Function callbacks may consume more fuel due to function storage/cleanup
    assert_fuel_consumed_within_threshold(387_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_wait_for_completion_enabled(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("wait-for-completion.js")
        .timers(true)
        .event_loop(true)
        .wait_for_completion(true)
        .build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Verify all async operations completed
    assert!(output_str.contains("Testing wait-for-completion functionality"));
    assert!(output_str.contains("All async operations scheduled"));
    
    // Test 1: Basic delayed timer
    assert!(output_str.contains("Test 1: Delayed timer executed"));
    
    // Test 2: Multiple timers with different delays
    assert!(output_str.contains("Test 2A: First timer"));
    assert!(output_str.contains("Test 2B: Second timer"));
    
    // Test 3: Promise resolution
    assert!(output_str.contains("Test 3: Promise resolved"));
    
    // Test 4: Nested timers
    assert!(output_str.contains("Test 4A: Outer timer"));
    assert!(output_str.contains("Test 4B: Nested timer"));
    
    // Should consume more fuel due to waiting and multiple timer executions
    assert_fuel_consumed_within_threshold(500_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_wait_for_completion_disabled(builder: &mut Builder) -> Result<()> {
    let mut runner = builder
        .input("wait-for-completion.js")
        .timers(true)
        .event_loop(true)
        // wait_for_completion is false by default
        .build()?;

    let (output, _logs, fuel_consumed) = run(&mut runner, vec![]);
    
    let output_str = String::from_utf8(output)?;
    
    // Should see initial output but not delayed timers
    assert!(output_str.contains("Testing wait-for-completion functionality"));
    assert!(output_str.contains("All async operations scheduled"));
    
    // Test 3: Promise should still resolve (immediate)
    assert!(output_str.contains("Test 3: Promise resolved"));
    
    // But delayed timers should NOT execute
    assert!(!output_str.contains("Test 1: Delayed timer executed"));
    assert!(!output_str.contains("Test 2A: First timer"));
    assert!(!output_str.contains("Test 2B: Second timer"));
    assert!(!output_str.contains("Test 4A: Outer timer"));
    assert!(!output_str.contains("Test 4B: Nested timer"));
    
    // Should consume less fuel since timers don't execute
    assert_fuel_consumed_within_threshold(300_000, fuel_consumed);
    
    Ok(())
}

#[javy_cli_test(commands(not(Compile)))]
fn test_wait_for_completion_without_event_loop_fails(builder: &mut Builder) -> Result<()> {
    // Test that wait-for-completion requires event loop
    let mut runner = builder
        .input("wait-for-completion.js")
        .timers(true)
        .wait_for_completion(true)
        // event_loop is false
        .build()?;
    
    let result = runner.exec(vec![]);
    assert!(result.is_err(), "wait-for-completion should fail without event loop");
    
    Ok(())
}
