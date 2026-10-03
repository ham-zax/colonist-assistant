use std::sync::atomic::{AtomicU32, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Host-owned cancellation word. The threaded browser engine shares its WASM
/// memory with the offscreen document, which writes this word directly so a
/// stale search stops at its next cooperative check instead of running out its
/// budget. Nothing in the engine writes it; hosts that never set it are
/// unaffected.
static SEARCH_CANCEL: AtomicU32 = AtomicU32::new(0);

/// Byte address of the cancellation word inside the module's linear memory.
pub fn search_cancel_word_address() -> usize {
    std::ptr::from_ref(&SEARCH_CANCEL) as usize
}

pub fn search_cancel_requested() -> bool {
    SEARCH_CANCEL.load(Ordering::Relaxed) != 0
}

#[cfg(target_arch = "wasm32")]
fn browser_now_ms() -> f64 {
    web_sys::window()
        .and_then(|window| window.performance())
        .map(|performance| performance.now())
        .unwrap_or_else(js_sys::Date::now)
}

/// Browser-safe cooperative wall-clock deadline shared by every search family.
///
/// `std::time::Instant::now()` traps on `wasm32-unknown-unknown`, so packaged
/// searches use the browser clock while native arena/tests retain `Instant`.
#[derive(Clone)]
pub struct CooperativeDeadline {
    budget_ms: u32,
    #[cfg(not(target_arch = "wasm32"))]
    started_at: Instant,
    #[cfg(target_arch = "wasm32")]
    started_at_ms: f64,
}

impl CooperativeDeadline {
    pub fn start(budget_ms: u32) -> Self {
        Self {
            budget_ms,
            #[cfg(not(target_arch = "wasm32"))]
            started_at: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            started_at_ms: browser_now_ms(),
        }
    }

    pub(crate) fn with_budget_ms(&self, budget_ms: u32) -> Self {
        let mut deadline = self.clone();
        deadline.budget_ms = budget_ms;
        deadline
    }

    pub fn has_elapsed(&self) -> bool {
        if search_cancel_requested() {
            return true;
        }
        if self.budget_ms == 0 {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started_at.elapsed().as_millis() >= u128::from(self.budget_ms)
        }
        #[cfg(target_arch = "wasm32")]
        {
            browser_now_ms() - self.started_at_ms >= f64::from(self.budget_ms)
        }
    }

    pub(crate) fn elapsed_ms(&self) -> u32 {
        #[cfg(not(target_arch = "wasm32"))]
        let elapsed = self
            .started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u32::MAX)) as u32;
        #[cfg(target_arch = "wasm32")]
        let elapsed = (browser_now_ms() - self.started_at_ms)
            .max(0.0)
            .min(f64::from(u32::MAX)) as u32;
        elapsed
    }

    pub fn remaining_ms(&self) -> u32 {
        if search_cancel_requested() {
            return 0;
        }
        if self.budget_ms == 0 {
            return u32::MAX;
        }
        self.budget_ms.saturating_sub(self.elapsed_ms())
    }

    #[cfg(test)]
    pub(crate) fn budget_ms_for_test(&self) -> u32 {
        self.budget_ms
    }

    pub(crate) fn expired_at_checkpoint(&self, completed_units: u32, interval: u32) -> bool {
        completed_units > 0 && completed_units.is_multiple_of(interval.max(1)) && self.has_elapsed()
    }
}
