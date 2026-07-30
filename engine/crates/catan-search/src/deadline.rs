#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// Browser-safe cooperative wall-clock deadline shared by every search family.
///
/// `std::time::Instant::now()` traps on `wasm32-unknown-unknown`, so packaged
/// searches use the browser clock while native arena/tests retain `Instant`.
#[derive(Clone)]
pub(crate) struct CooperativeDeadline {
    budget_ms: u32,
    #[cfg(not(target_arch = "wasm32"))]
    started_at: Instant,
    #[cfg(target_arch = "wasm32")]
    started_at_ms: f64,
}

impl CooperativeDeadline {
    pub(crate) fn start(budget_ms: u32) -> Self {
        Self {
            budget_ms,
            #[cfg(not(target_arch = "wasm32"))]
            started_at: Instant::now(),
            #[cfg(target_arch = "wasm32")]
            started_at_ms: js_sys::Date::now(),
        }
    }

    pub(crate) fn has_elapsed(&self) -> bool {
        if self.budget_ms == 0 {
            return false;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started_at.elapsed().as_millis() >= u128::from(self.budget_ms)
        }
        #[cfg(target_arch = "wasm32")]
        {
            js_sys::Date::now() - self.started_at_ms >= f64::from(self.budget_ms)
        }
    }

    pub(crate) fn expired_at_checkpoint(&self, completed_units: u32, interval: u32) -> bool {
        completed_units > 0 && completed_units.is_multiple_of(interval.max(1)) && self.has_elapsed()
    }
}
