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
            // Check that API is available
            assert_eq!("function", cx.eval::<String, _>("typeof setTimeout")?);
            assert_eq!("function", cx.eval::<String, _>("typeof clearTimeout")?);
            assert_eq!("function", cx.eval::<String, _>("typeof setInterval")?);
            assert_eq!("function", cx.eval::<String, _>("typeof clearInterval")?);
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
            let code = r#"const id = setTimeout('console.log("test")', 1000); clearTimeout(id); id"#;
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

        runtime.context().with(|cx| {
            cx.eval::<(), _>("globalThis.var1 = -123; setTimeout('globalThis.var1 = 321', 0)")?;
            Ok::<_, Error>(())
        })?;

        // Process timers immediately without sleep - they should be available
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was executed
            assert_eq!(321, cx.eval::<i32, _>("globalThis.var1")?);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_timer_function_callback() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            // Create timeout with a closure with a mutable state
            // To make sure the closure preserves the state reference
            cx.eval::<(), _>("
                globalThis.var1 = -123;
                function createIncrementor(initialDelta) {
                    let delta = initialDelta;
                    return [function() { globalThis.var1 += delta; }, function(newDelta) { delta = newDelta; }];
                }
                [incrementor, setDelta] = createIncrementor(100);
                incrementor();
                setTimeout(incrementor, 0);
                setDelta(123);
            ")?;

            // So far, only explicit call to incrementor (having delta = 100) is done
            assert_eq!(-23, cx.eval::<i32, _>("globalThis.var1")?);

            Ok::<_, Error>(())
        })?;

        // Process timers immediately without sleep - they should be available
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if closure correctly applied delta modified after its creation
            assert_eq!(100, cx.eval::<i32, _>("globalThis.var1")?);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_timer_with_delay() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            // Set a timer with a delay that shouldn't fire immediately
            cx.eval::<(), _>("globalThis.var1 = -765; setTimeout('globalThis.var1 = 567', 1000)")?;
            Ok::<_, Error>(())
        })?;

        // Process timers immediately - should not execute
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was NOT executed
            assert_eq!(-765, cx.eval::<i32, _>("globalThis.var1")?);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_multiple_timers() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            // Set multiple timers
            cx.eval::<(), _>("
                globalThis.var1 = 0;
                globalThis.var2 = 0;
                setTimeout('globalThis.var1 = 123', 0);
                setTimeout('globalThis.var2 = 321', 0);
            ")?;
            Ok::<_, Error>(())
        })?;

        // Process timers
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if both timers were executed
            assert_eq!(123, cx.eval::<i32, _>("globalThis.var1")?);
            assert_eq!(321, cx.eval::<i32, _>("globalThis.var2")?);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_clear_timeout_removes_timer() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            // Set a timer and immediately clear it
            cx.eval::<(), _>("
                globalThis.var1 = -432;
                const id = setTimeout('globalThis.var1 = 234', 0);
                clearTimeout(id);
            ")?;
            Ok::<_, Error>(())
        })?;

        // Process timers
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if timer was NOT executed
            assert_eq!(-432, cx.eval::<i32, _>("globalThis.var1")?);
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
            cx.eval::<(), _>(r#"setTimeout('console.log("test")', 1000)"#)?;
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
            let code = r#"const id = setInterval('console.log("test")', 1000); clearInterval(id); id"#;
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

        runtime.context().with(|cx| {
            cx.eval::<(), _>("globalThis.var1 = 1000; setInterval('globalThis.var1++', 0)")?;
            Ok::<_, Error>(())
        })?;

        // Process timers multiple times to test rescheduling
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check if interval executed multiple times (showing it's repeating)
            let var1: i32 = cx.eval("globalThis.var1")?;
            assert!(var1 >= 1002, "Interval should have executed multiple times, got {}", var1);
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_clear_interval_stops_repetition() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            cx.eval::<(), _>("
                globalThis.var1 = 100;
                const id = setInterval('globalThis.var1++', 0);
                clearInterval(id);
            ")?;
            Ok::<_, Error>(())
        })?;

        // Process timers multiple times
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            // Check that interval was NOT executed (should be 0)
            let var1: i32 = cx.eval("globalThis.var1")?;
            assert_eq!(100, var1, "Cleared interval should not execute");
            Ok::<_, Error>(())
        })?;
        Ok(())
    }

    #[test]
    fn test_interval_and_timeout_coexistence() -> Result<()> {
        let mut config = Config::default();
        config.timers(true);
        let runtime = Runtime::new(config)?;

        runtime.context().with(|cx| {
            // Set timeout first, then interval - both with 0 delay
            cx.eval::<(), _>("
                globalThis.var1 = -543;
                globalThis.var2 = 100;
                setTimeout('globalThis.var1 = 999', 0);
                setInterval('globalThis.var2++', 0);
            ")?;
            Ok::<_, Error>(())
        })?;

        // Process timers first time - should execute both timeout and first interval
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            let var1: i32 = cx.eval("globalThis.var1")?;
            let var2: i32 = cx.eval("globalThis.var2")?;

            assert_eq!(999, var1, "Timeout should have executed");
            assert!(var2 >= 101, "Interval should have executed at least once, got {}", var2);

            Ok::<_, Error>(())
        })?;

        // Process timers again to verify interval repeats (timeout shouldn't run again)
        runtime.resolve_pending_jobs()?;

        runtime.context().with(|cx| {
            let var1: i32 = cx.eval("globalThis.var1")?;
            let var2: i32 = cx.eval("globalThis.var2")?;

            // Timeout should still be true (unchanged), interval should have incremented
            assert_eq!(999, var1, "Timeout should remain executed");
            assert!(var2 >= 102, "Interval should have executed multiple times");

            Ok::<_, Error>(())
        })?;
        Ok(())
    }
}
