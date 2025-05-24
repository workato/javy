use std::{
    collections::BinaryHeap,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub(super) enum TimerCallback {
    Code(String),
    Function,
}

/// Timer entry in the timer queue
#[derive(Debug)]
pub(super) struct Timer {
    pub id: u32,
    pub fire_time: u64,           // milliseconds since UNIX epoch
    pub callback: TimerCallback,
    pub interval_ms: Option<u32>, // If Some(), this is a repeating timer
}

impl PartialEq for Timer {
    fn eq(&self, other: &Self) -> bool {
        self.fire_time == other.fire_time
    }
}

impl Eq for Timer {}

impl PartialOrd for Timer {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timer {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse order for min-heap behavior
        other.fire_time.cmp(&self.fire_time)
    }
}

/// Global timer queue
#[derive(Debug)]
pub(super) struct TimerQueue {
    timers: BinaryHeap<Timer>,
    next_id: u32,
}

impl TimerQueue {
    pub fn new() -> Self {
        Self {
            timers: BinaryHeap::new(),
            next_id: 1,
        }
    }

    pub fn add_timer(
        &mut self,
        delay_ms: u32,
        repeat: bool,
        callback: TimerCallback,
        reuse_id: Option<u32>,
    ) -> u32 {
        let now = Self::now();

        let id = reuse_id.unwrap_or_else(|| {
            let id = self.next_id;
            self.next_id += 1;
            id
        });

        let timer = Timer {
            id,
            fire_time: now + delay_ms as u64,
            callback,
            interval_ms: if repeat { Some(delay_ms) } else { None },
        };

        self.timers.push(timer);
        id
    }

    pub fn remove_timer(&mut self, timer_id: u32) -> bool {
        let original_len = self.timers.len();
        self.timers.retain(|timer| timer.id != timer_id);
        self.timers.len() != original_len
    }

    pub fn get_expired_timers(&mut self) -> Vec<Timer> {
        let now = Self::now();
        let mut expired = Vec::new();
        while let Some(timer) = self.timers.peek() {
            if timer.fire_time <= now {
                expired.push(self.timers.pop().unwrap());
            } else {
                break;
            }
        }

        expired
    }

    pub fn has_pending_timers(&self) -> bool {
        !self.timers.is_empty()
    }

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_queue() {
        let mut queue = TimerQueue::new();

        fn add_timer(delay_ms: u32, callback_code: &str, queue: &mut TimerQueue) -> u32 {
            queue.add_timer(delay_ms, false, TimerCallback::Code(callback_code.to_string()), None)
        }

        // Add some timers
        let id1 = add_timer(100, "console.log('timer1')", &mut queue);
        let id2 = add_timer(50, "console.log('timer2')", &mut queue);
        let id3 = add_timer(200, "console.log('timer3')", &mut queue);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);

        assert!(queue.has_pending_timers());

        // Remove a timer
        assert!(queue.remove_timer(id2));
        assert!(!queue.remove_timer(999)); // Non-existent timer

        assert!(queue.has_pending_timers());
    }
}
