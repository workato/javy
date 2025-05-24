use std::sync::{Arc, Mutex};

mod queue;
use queue::TimerQueue;

use crate::{
    hold, hold_and_release,
    quickjs::{prelude::MutFn, Ctx, Function, Value},
    to_js_error, val_to_string, Args,
};
use anyhow::{anyhow, Result};

pub struct TimersRuntime {
    queue: Arc<Mutex<TimerQueue>>,
}

impl TimersRuntime {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(TimerQueue::new())),
        }
    }

    /// Register timer functions on the global object
    pub fn register_globals(&self, this: Ctx<'_>) -> Result<()> {
        let globals = this.globals();

        let queue = self.queue.clone();
        globals.set("setTimeout", Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            set_timeout(&queue, hold!(cx.clone(), args))
                .map_err(|e| to_js_error(cx, e))
        }))?)?;

        let queue = self.queue.clone();
        globals.set("clearTimeout",Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            clear_timeout(&queue, hold!(cx.clone(), args))
                .map_err(|e| to_js_error(cx, e))
        }))?)?;

        let queue = self.queue.clone();
        globals.set("setInterval", Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            set_interval(&queue, hold!(cx.clone(), args))
                .map_err(|e| to_js_error(cx, e))
        }))?)?;

        let queue = self.queue.clone();
        globals.set("clearInterval", Function::new(this.clone(), MutFn::new(move |cx, args| {
            let (cx, args) = hold_and_release!(cx, args);
            clear_interval(&queue, hold!(cx.clone(), args))
                .map_err(|e| to_js_error(cx, e))
        }))?)?;

        Ok(())
    }

    /// Process expired timers - should be called by the event loop
    pub fn process_timers(&self, ctx: Ctx<'_>) -> Result<()> {
        let mut queue = self.queue.lock().unwrap();
        let expired_timers = queue.get_expired_timers();

        // Reschedule intervals before releasing the lock
        for timer in &expired_timers {
            if let Some(interval_ms) = timer.interval_ms {
                queue.add_timer(interval_ms, true, timer.callback.clone(), Some(timer.id));
            }
        }

        drop(queue); // Release lock before executing JavaScript

        // Execute all timer callbacks (both timeouts and intervals)
        for timer in &expired_timers {
            if let Err(e) = ctx.eval::<(), _>(timer.callback.as_str()) {
                eprintln!("Timer callback error: {}", e);
            }
        }

        Ok(())
    }

    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        let queue = self.queue.lock().unwrap();
        queue.has_pending_timers()
    }
}

fn set_timeout<'js>(queue: &Arc<Mutex<TimerQueue>>, args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("setTimeout requires at least 1 argument"));
    }

    // Get callback (can be function or string)
    let callback_str = if args[0].is_function() {
        // Convert function to string representation
        val_to_string(&ctx, args[0].clone())?
    } else {
        // Treat as string code
        val_to_string(&ctx, args[0].clone())?
    };

    // Get delay (default to 0 if not provided)
    let delay_ms = if args.len() > 1 {
        args[1].as_number().unwrap_or(0.0).max(0.0) as u32
    } else {
        0
    };

    let mut queue = queue.lock().unwrap();
    let timer_id = queue.add_timer(delay_ms, false, callback_str, None);
    drop(queue);

    Ok(Value::new_int(ctx, timer_id as i32))
}

fn clear_timeout<'js>(queue: &Arc<Mutex<TimerQueue>>, args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Ok(Value::new_undefined(ctx));
    }

    let timer_id = args[0].as_number().unwrap_or(0.0) as u32;

    let mut queue = queue.lock().unwrap();
    queue.remove_timer(timer_id);
    drop(queue);

    Ok(Value::new_undefined(ctx))
}

fn set_interval<'js>(queue: &Arc<Mutex<TimerQueue>>, args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Err(anyhow!("setInterval requires at least 1 argument"));
    }

    // Get callback (can be function or string)
    let callback_str = if args[0].is_function() {
        // Convert function to string representation
        val_to_string(&ctx, args[0].clone())?
    } else {
        // Treat as string code
        val_to_string(&ctx, args[0].clone())?
    };

    // Get interval (default to 0 if not provided)
    let interval_ms = if args.len() > 1 {
        args[1].as_number().unwrap_or(0.0).max(0.0) as u32
    } else {
        0
    };

    let mut queue = queue.lock().unwrap();
    let timer_id = queue.add_timer(interval_ms, true, callback_str, None);
    drop(queue);

    Ok(Value::new_int(ctx, timer_id as i32))
}

fn clear_interval<'js>(queue: &Arc<Mutex<TimerQueue>>, args: Args<'js>) -> Result<Value<'js>> {
    let (ctx, args) = args.release();
    let args = args.into_inner();

    if args.is_empty() {
        return Ok(Value::new_undefined(ctx));
    }

    let timer_id = args[0].as_number().unwrap_or(0.0) as u32;

    let mut queue = queue.lock().unwrap();
    queue.remove_timer(timer_id);
    drop(queue);

    Ok(Value::new_undefined(ctx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Config, Runtime};
    use anyhow::Error;

    #[test]
    fn test_register() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            // Check that setTimeout is available
            let type_str: String = cx.eval("typeof setTimeout")?;
            assert_eq!(type_str, "function");

            // Check that clearTimeout is available
            let type_str: String = cx.eval("typeof clearTimeout")?;
            assert_eq!(type_str, "function");

            // Check that setInterval is available
            let type_str: String = cx.eval("typeof setInterval")?;
            assert_eq!(type_str, "function");

            // Check that clearInterval is available
            let type_str: String = cx.eval("typeof clearInterval")?;
            assert_eq!(type_str, "function");

            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_set_timeout_basic() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            // Test setTimeout with string callback
            let timer_id: i32 = cx.eval("setTimeout('1+1', 100)")?;
            assert!(timer_id > 0);

            // Test setTimeout with function callback
            let timer_id2: i32 = cx.eval("setTimeout(function() { return 42; }, 50)")?;
            assert!(timer_id2 > timer_id);

            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_clear_timeout() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            // Create a timer and clear it
            let code = "const id = setTimeout('console.log(\"test\")', 1000); clearTimeout(id); id";
            let timer_id: i32 = cx.eval(code)?;
            assert!(timer_id > 0);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_timer_execution() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use a unique variable name to avoid interference between tests
        let unique_var = format!("timerExecuted_{}", std::process::id());

        runtime.context().with(|cx| {
            cx.eval::<(), _>(format!(
                "globalThis.{} = false; setTimeout('globalThis.{} = true', 0)",
                unique_var, unique_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers immediately without sleep - they should be available
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was executed
            let result: bool = cx.eval(format!("globalThis.{}", unique_var))?;
            assert!(result);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_timer_with_delay() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable name to avoid interference between tests
        let unique_var = format!("delayedTimer_{}", std::process::id());

        runtime.context().with(|cx| {
            // Set a timer with a delay that shouldn't fire immediately
            cx.eval::<(), _>(format!(
                "globalThis.{} = false; setTimeout('globalThis.{} = true', 1000)",
                unique_var, unique_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers immediately - should not execute
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was NOT executed
            let result: bool = cx.eval(format!("globalThis.{}", unique_var))?;
            assert_eq!(result, false);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_multiple_timers() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable names to avoid interference between tests
        let unique_id = std::process::id();
        let timer1_var = format!("timer1_{}", unique_id);
        let timer2_var = format!("timer2_{}", unique_id);

        runtime.context().with(|cx| {
            // Set multiple timers
            cx.eval::<(), _>(format!(
                "
                    globalThis.{} = false;
                    globalThis.{} = false;
                    setTimeout('globalThis.{} = true', 0);
                    setTimeout('globalThis.{} = true', 0);
                ",
                timer1_var, timer2_var, timer1_var, timer2_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if both timers were executed
            let result1: bool = cx.eval(format!("globalThis.{}", timer1_var))?;
            let result2: bool = cx.eval(format!("globalThis.{}", timer2_var))?;
            assert_eq!(result1, true);
            assert_eq!(result2, true);

            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_clear_timeout_removes_timer() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable name to avoid interference between tests
        let unique_var = format!("clearedTimer_{}", std::process::id());

        runtime.context().with(|cx| {
            // Set a timer and immediately clear it
            cx.eval::<(), _>(format!(
                "
                    globalThis.{} = false;
                    const id = setTimeout('globalThis.{} = true', 0);
                    clearTimeout(id);
                ",
                unique_var, unique_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was NOT executed
            let result: bool = cx.eval(format!("globalThis.{}", unique_var))?;
            assert_eq!(result, false);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_has_pending_timers() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Initially no pending timers
        assert!(!runtime.has_pending_timers());

        runtime.context().with(|cx| {
            // Add a timer
            cx.eval::<(), _>("setTimeout('console.log(\"test\")', 1000)")?;
            Ok::<_, Error>(())
        })?;

        // Should have pending timers
        assert!(runtime.has_pending_timers());

        Ok(())
    }

    #[test]
    fn test_set_interval_basic() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            // Test setInterval with string callback
            let interval_id: i32 = cx.eval("setInterval('1+1', 100)")?;
            assert!(interval_id > 0);
            Ok::<_, Error>(())
        })?;

        // Should have pending timers
        assert!(runtime.has_pending_timers());

        Ok(())
    }

    #[test]
    fn test_clear_interval() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;
        runtime.context().with(|cx| {
            // Create an interval and clear it
            let code = "const id = setInterval('console.log(\"test\")', 1000); clearInterval(id); id";
            let interval_id: i32 = cx.eval(code)?;
            assert!(interval_id > 0);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_interval_execution_and_rescheduling() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable name to avoid interference between tests
        let unique_var = format!("intervalCount_{}", std::process::id());

        runtime.context().with(|cx| {
            cx.eval::<(), _>(format!(
                "globalThis.{} = 0; setInterval('globalThis.{}++', 0)",
                unique_var, unique_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers multiple times to test rescheduling
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if interval executed multiple times (showing it's repeating)
            let count: i32 = cx.eval(format!("globalThis.{}", unique_var))?;
            assert!(count >= 2, "Interval should have executed multiple times, got {}", count);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_clear_interval_stops_repetition() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable name to avoid interference between tests
        let unique_var = format!("clearedIntervalCount_{}", std::process::id());

        runtime.context().with(|cx| {
            cx.eval::<(), _>(format!(
                "
                    globalThis.{} = 0;
                    const id = setInterval('globalThis.{}++', 0);
                    clearInterval(id);
                ",
                unique_var, unique_var
            ))?;
            Ok::<_, Error>(())
        })?;

        // Process timers multiple times
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check that interval was NOT executed (should be 0)
            let count: i32 = cx.eval(format!("globalThis.{}", unique_var))?;
            assert_eq!(count, 0, "Cleared interval should not execute");
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_interval_and_timeout_coexistence() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        // Use unique variable names
        let unique_id = std::process::id();
        let timeout_var = format!("timeoutExecuted_{}", unique_id);
        let interval_var = format!("intervalCount_{}", unique_id);

        runtime.context().with(|cx| {
            // Set timeout first, then interval - both with 0 delay
            let timer_code = format!(
                "
                    globalThis.{} = false;
                    globalThis.{} = 0;
                    setTimeout('globalThis.{} = true', 0);
                    setInterval('globalThis.{}++', 0);
                ",
                timeout_var, interval_var, timeout_var, interval_var
            );
            cx.eval::<(), _>(timer_code)?;
            Ok::<_, Error>(())
        })?;

        // Process timers first time - should execute both timeout and first interval
        runtime.resolve_pending_jobs()?;

        // Check both timeout and interval results
        let timeout_check = format!("globalThis.{}", timeout_var);
        let interval_check = format!("globalThis.{}", interval_var);

        runtime.context().with(|cx| {
            let timeout_result: bool = cx.eval(timeout_check.as_str())?;
            let interval_result: i32 = cx.eval(interval_check.as_str())?;

            assert!(timeout_result, "Timeout should have executed");
            assert!(interval_result >= 1, "Interval should have executed at least once");

            Ok::<_, Error>(())
        })?;

        // Process timers again to verify interval repeats (timeout shouldn't run again)
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            let timeout_result: bool = cx.eval(timeout_check.as_str())?;
            let interval_result: i32 = cx.eval(interval_check.as_str())?;

            // Timeout should still be true (unchanged), interval should have incremented
            assert!(timeout_result, "Timeout should remain executed");
            assert!(interval_result >= 2, "Interval should have executed multiple times");

            Ok::<_, Error>(())
        })?;
        Ok(())
    }
}
