#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

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
        let elapsed = self.started_at.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
        #[cfg(target_arch = "wasm32")]
        let elapsed = (browser_now_ms() - self.started_at_ms)
            .max(0.0)
            .min(f64::from(u32::MAX)) as u32;
        elapsed
    }

    pub fn remaining_ms(&self) -> u32 {
        if self.budget_ms == 0 {
            return u32::MAX;
        }
        self.budget_ms.saturating_sub(self.elapsed_ms())
    }

    pub(crate) fn expired_at_checkpoint(&self, completed_units: u32, interval: u32) -> bool {
        completed_units > 0 && completed_units.is_multiple_of(interval.max(1)) && self.has_elapsed()
    }
}
