//! "Latest frame" hand-off to a dedicated worker thread (used by serial and USB sinks).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use super::Stat;

pub struct Slot {
    frame: Mutex<Option<Box<[u8; 512]>>>,
    cv: Condvar,
    stop: AtomicBool,
}

impl Slot {
    pub fn new() -> Arc<Self> {
        Arc::new(Slot { frame: Mutex::new(None), cv: Condvar::new(), stop: AtomicBool::new(false) })
    }

    /// Never blocks (the lock is only held for a 512-byte copy).
    pub fn put(&self, data: &[u8; 512]) {
        let mut g = self.frame.lock();
        match g.as_mut() {
            Some(b) => **b = *data,
            None => *g = Some(Box::new(*data)),
        }
        self.cv.notify_one();
    }

    pub fn close(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.cv.notify_all();
    }

    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Wait up to `timeout` for a new frame (returns early on `put`/`close`).
    pub fn wait(&self, timeout: Duration) {
        let mut g = self.frame.lock();
        let _ = self.cv.wait_for(&mut g, timeout);
    }

    pub fn latest(&self) -> Option<[u8; 512]> {
        self.frame.lock().as_ref().map(|b| **b)
    }

    /// Sleep that wakes early when closed; returns true while still running.
    pub fn sleep(&self, d: Duration) -> bool {
        let end = Instant::now() + d;
        while Instant::now() < end {
            if self.stopped() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20).min(end.saturating_duration_since(Instant::now())));
        }
        !self.stopped()
    }
}

pub type SharedStat = Arc<Mutex<Stat>>;

/// Minimum spacing between full DMX frames (≈ 44 Hz, the physical limit of a 512-slot frame).
pub const MIN_FRAME_GAP: Duration = Duration::from_millis(23);
