//! A crate for creating Javy plugins
//!
//! Example usage:
//! ```rust
//! use javy_plugin_api::import_namespace;
//! use javy_plugin_api::Config;
//!
//! // Dynamically linked modules will use `my_javy_plugin_v1` as the import
//! // namespace.
//! import_namespace!("my_javy_plugin_v1");
//!
//! #[export_name = "initialize_runtime"]
//! pub extern "C" fn initialize_runtime() {
//!    let mut config = Config::default();
//!    config
//!        .text_encoding(true)
//!        .javy_stream_io(true);
//!
//!    javy_plugin_api::initialize_runtime(config, |runtime| runtime).unwrap();
//! }
//! ```
//!
//! The crate will automatically add exports for a number of Wasm functions in
//! your crate that Javy needs to work.
//!
//! # Core concepts
//! * [`javy`] - a re-export of the [`javy`] crate.
//! * [`import_namespace`] - required to provide an import namespace when the
//!   plugin is used to generate dynamically linked modules.
//! * [`initialize_runtime`] - used to configure the QuickJS runtime with a
//!   [`Config`] to add behavior to the created [`javy::Runtime`].
//!
//! # Features
//! * `json` - enables the `json` feature in the `javy` crate.
//! * `messagepack` - enables the `messagepack` feature in the `javy` crate.

// Allow these in this file because we only run this program single threaded
// and we can safely reason about the accesses to the Javy Runtime. We also
// don't want to introduce overhead from taking unnecessary mutex locks.
#![allow(static_mut_refs)]
use anyhow::{anyhow, bail, Error, Result};
pub use config::Config;
use javy::quickjs::{self, Ctx, Error as JSError, Function, Module, Value};
use javy::{from_js_error, Runtime};
use std::cell::OnceCell;
use std::{process, slice, str};

pub use javy;

mod config;
mod namespace;

const FUNCTION_MODULE_NAME: &str = "function.mjs";

static mut COMPILE_SRC_RET_AREA: [u32; 2] = [0; 2];

static mut RUNTIME: OnceCell<Runtime> = OnceCell::new();
static mut EVENT_LOOP_ENABLED: bool = false;
static mut WAIT_FOR_COMPLETION: bool = false;
static mut WAIT_TIMEOUT_MS: Option<u64> = None;

static EVENT_LOOP_ERR: &str = r#"
                Pending jobs in the event queue.
                Scheduling events is not supported when the 
                event-loop runtime config is not enabled.
            "#;

/// Initializes the Javy runtime.
pub fn initialize_runtime<F>(config: Config, modify_runtime: F) -> Result<()>
where
    F: FnOnce(Runtime) -> Runtime,
{
    // Validate configuration dependencies
    if config.wait_for_completion && !config.event_loop {
        bail!("wait_for_completion requires event_loop to be enabled");
    }
    
    let runtime = Runtime::new(config.runtime_config).unwrap();
    let runtime = modify_runtime(runtime);
    unsafe {
        RUNTIME.take(); // Allow re-initializing.
        RUNTIME
            .set(runtime)
            // `unwrap` requires error `T` to implement `Debug` but `set`
            // returns the `javy::Runtime` on error and `javy::Runtime` does not
            // implement `Debug`.
            .map_err(|_| anyhow!("Could not pre-initialize javy::Runtime"))
            .unwrap();
        EVENT_LOOP_ENABLED = config.event_loop;
        WAIT_FOR_COMPLETION = config.wait_for_completion;
        WAIT_TIMEOUT_MS = config.wait_timeout_ms;
    };
    Ok(())
}

/// Compiles JS source code to QuickJS bytecode.
///
/// Returns a pointer to a buffer containing a 32-bit pointer to the bytecode byte array and the
/// u32 length of the bytecode byte array.
///
/// # Arguments
///
/// * `js_src_ptr` - A pointer to the start of a byte array containing UTF-8 JS source code
/// * `js_src_len` - The length of the byte array containing JS source code
///
/// # Safety
///
/// * `js_src_ptr` must reference a valid array of unsigned bytes of `js_src_len` length
#[export_name = "compile_src"]
pub unsafe extern "C" fn compile_src(js_src_ptr: *const u8, js_src_len: usize) -> *const u32 {
    // Use initialized runtime when compiling because certain runtime
    // configurations can cause different bytecode to be emitted.
    //
    // For example, given the following JS:
    // ```
    // function foo() {
    //   "use math"
    //   1234 % 32
    // }
    // ```
    //
    // Setting `config.bignum_extension` to `true` will produce different
    // bytecode than if it were set to `false`.
    let runtime = unsafe { RUNTIME.get().unwrap() };
    let js_src = str::from_utf8(slice::from_raw_parts(js_src_ptr, js_src_len)).unwrap();

    let bytecode = runtime
        .compile_to_bytecode(FUNCTION_MODULE_NAME, js_src)
        .unwrap();

    // We need the bytecode buffer to live longer than this function so it can be read from memory
    let len = bytecode.len();
    let bytecode_ptr = Box::leak(bytecode.into_boxed_slice()).as_ptr();
    COMPILE_SRC_RET_AREA[0] = bytecode_ptr as u32;
    COMPILE_SRC_RET_AREA[1] = len.try_into().unwrap();
    COMPILE_SRC_RET_AREA.as_ptr()
}

/// Evaluates QuickJS bytecode and optionally invokes exported JS function with
/// name.
///
/// # Safety
///
/// * `bytecode_ptr` must reference a valid array of bytes of `bytecode_len`
///   length.
/// * If `fn_name_ptr` is not 0, it must reference a UTF-8 string with
///   `fn_name_len` byte length.
#[export_name = "invoke"]
pub unsafe extern "C" fn invoke(
    bytecode_ptr: *const u8,
    bytecode_len: usize,
    fn_name_ptr: *const u8,
    fn_name_len: usize,
) {
    let bytecode = slice::from_raw_parts(bytecode_ptr, bytecode_len);
    let fn_name = if !fn_name_ptr.is_null() && fn_name_len != 0 {
        Some(str::from_utf8_unchecked(slice::from_raw_parts(
            fn_name_ptr,
            fn_name_len,
        )))
    } else {
        None
    };
    run_bytecode(bytecode, fn_name);
}

/// Evaluate the given bytecode.
///
/// Deprecated for use outside of this crate.
///
/// Evaluating also prepares (or "instantiates") the state of the JavaScript
/// engine given all the information encoded in the bytecode.
pub fn run_bytecode(bytecode: &[u8], fn_name: Option<&str>) {
    let runtime = unsafe { RUNTIME.get() }.unwrap();
    runtime
        .context()
        .with(|this| {
            let module = unsafe { Module::load(this.clone(), bytecode)? };
            let (module, promise) = module.eval()?;

            handle_maybe_promise(this.clone(), promise.into())?;

            if let Some(fn_name) = fn_name {
                let fun: Function = module.get(fn_name)?;
                // Exported functions are guaranteed not to have arguments so
                // we can safely pass an empty tuple for arguments.
                let value = fun.call(())?;
                handle_maybe_promise(this.clone(), value)?
            }
            Ok(())
        })
        .map_err(|e| runtime.context().with(|cx| from_js_error(cx.clone(), e)))
        .and_then(|_: ()| ensure_pending_jobs(runtime))
        .unwrap_or_else(handle_error)
}

/// Handles the promise returned by evaluating the JS bytecode.
fn handle_maybe_promise(this: Ctx, value: Value) -> quickjs::Result<()> {
    match value.as_promise() {
        Some(promise) => {
            if unsafe { EVENT_LOOP_ENABLED } {
                // If the event loop is enabled, trigger it.
                let resolved = promise.finish::<Value>();
                // `Promise::finish` returns Err(Wouldblock) when the all
                // pending jobs have been handled.
                if let Err(JSError::WouldBlock) = resolved {
                    Ok(())
                } else {
                    resolved.map(|_| ())
                }
            } else {
                // Else we simply expect the promise to resolve immediately.
                match promise.result() {
                    None => Err(javy::to_js_error(this, anyhow!(EVENT_LOOP_ERR))),
                    Some(r) => r,
                }
            }
        }
        None => Ok(()),
    }
}

fn ensure_pending_jobs(rt: &Runtime) -> Result<()> {
    if unsafe { EVENT_LOOP_ENABLED } {
        if unsafe { WAIT_FOR_COMPLETION } {
            // Wait for all async operations to complete
            wait_for_completion(rt)
        } else {
            // Original behavior: resolve once
            rt.resolve_pending_jobs()
        }
    } else if rt.has_pending_jobs() {
        bail!(EVENT_LOOP_ERR);
    } else {
        Ok(())
    }
}

fn wait_for_completion(rt: &Runtime) -> Result<()> {
    use std::{thread, time::{Duration, Instant}};
    
    const SLEEP_MS: u64 = 1; // 1ms sleep between iterations
    
    let timeout_ms = unsafe { WAIT_TIMEOUT_MS };
    let start_time = Instant::now();
    
    loop {
        // Process any immediately available jobs
        rt.resolve_pending_jobs()?;
        
        // Check if there are still pending jobs
        if !rt.has_pending_jobs() {
            break;
        }
        
        // Check timeout if configured
        if let Some(timeout) = timeout_ms {
            let elapsed = start_time.elapsed().as_millis() as u64;
            if elapsed >= timeout {
                eprintln!("Warning: Timeout reached ({} ms) while waiting for async operations to complete", timeout);
                break;
            }
        }
        
        // Sleep briefly to allow time to pass for delayed timers
        thread::sleep(Duration::from_millis(SLEEP_MS));
    }
    
    Ok(())
}

fn handle_error(e: Error) {
    eprintln!("{e}");
    process::abort();
}

#[cfg(test)]
mod tests {
    use super::*;
    use javy::{Config as JavyConfig, Runtime};

    #[test]
    fn test_wait_for_completion_no_pending_jobs() {
        let javy_config = JavyConfig::default();
        let runtime = Runtime::new(javy_config).unwrap();
        
        let result = wait_for_completion(&runtime);
        assert!(result.is_ok());
    }

    #[test]
    fn test_wait_for_completion_with_immediate_timers() {
        let mut javy_config = JavyConfig::default();
        javy_config.timers(true);
        let runtime = Runtime::new(javy_config).unwrap();
        
        // Create a timer that executes immediately
        runtime.context().with(|cx| {
            cx.eval::<(), _>("setTimeout(() => { globalThis.testResult = 'completed'; }, 0)").unwrap();
        });
        
        let result = wait_for_completion(&runtime);
        assert!(result.is_ok());
        
        // Verify the timer executed
        runtime.context().with(|cx| {
            let result: String = cx.eval("globalThis.testResult || 'not executed'").unwrap();
            assert_eq!(result, "completed");
        });
    }

    #[test]
    fn test_ensure_pending_jobs_behavior() {
        let javy_config = JavyConfig::default();
        let runtime = Runtime::new(javy_config).unwrap();
        
        // Test with wait_for_completion disabled (default)
        let mut config1 = Config::default();
        config1.event_loop(true).wait_for_completion(false);
        initialize_runtime(config1, |rt| rt).unwrap();
        let result = ensure_pending_jobs(&runtime);
        assert!(result.is_ok());
        
        // Test with wait_for_completion enabled
        let mut config2 = Config::default();
        config2.event_loop(true).wait_for_completion(true);
        initialize_runtime(config2, |rt| rt).unwrap();
        let result = ensure_pending_jobs(&runtime);
        assert!(result.is_ok());
    }
}
