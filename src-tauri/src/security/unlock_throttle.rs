//! Progressive, in-memory cooldown policy for failed profile unlock attempts.

use std::time::{Duration, Instant};

/// Sets the initial cooldown and keeps repeated failures from becoming unbounded.
const INITIAL_COOLDOWN: Duration = Duration::from_millis(250);
/// Caps the delay while still making repeated online passphrase guessing impractical.
const MAXIMUM_COOLDOWN: Duration = Duration::from_secs(30);

/// Tracks unlock failures only for the running process and never persists authentication data.
pub(super) struct UnlockThrottle {
    failed_attempts: u32,
    next_allowed_at: Option<Instant>,
}

impl UnlockThrottle {
    /// Starts with no delay before the first unlock request.
    pub(super) fn new() -> Self {
        Self { failed_attempts: 0, next_allowed_at: None }
    }

    /// Returns whether an unlock attempt must wait without deriving a key or opening data.
    pub(super) fn is_blocked_at(&self, now: Instant) -> bool {
        self.next_allowed_at.is_some_and(|next_allowed_at| now < next_allowed_at)
    }

    /// Records a failed attempt and returns the newly enforced cooldown duration.
    pub(super) fn record_failure_at(&mut self, now: Instant) -> Duration {
        self.failed_attempts = self.failed_attempts.saturating_add(1);
        let cooldown = cooldown_for(self.failed_attempts);
        self.next_allowed_at = now.checked_add(cooldown);

        cooldown
    }

    /// Clears the failure history only after a successful authenticated unlock.
    pub(super) fn reset(&mut self) {
        self.failed_attempts = 0;
        self.next_allowed_at = None;
    }
}

/// Doubles the cooldown after each failure while preserving the defined maximum.
fn cooldown_for(failed_attempts: u32) -> Duration {
    let exponent = failed_attempts.saturating_sub(1).min(7);
    let multiplier = 1_u32 << exponent;
    let cooldown = INITIAL_COOLDOWN.saturating_mul(multiplier);

    cooldown.min(MAXIMUM_COOLDOWN)
}

#[cfg(test)]
mod tests {
    use super::UnlockThrottle;
    use std::time::{Duration, Instant};

    /// Proves each failure extends the delay and a successful unlock restores immediate access.
    #[test]
    fn progressively_throttles_failures_and_resets_after_success() {
        let mut throttle = UnlockThrottle::new();
        let started_at = Instant::now();

        assert_eq!(throttle.record_failure_at(started_at), Duration::from_millis(250));
        assert!(throttle.is_blocked_at(started_at));
        assert!(!throttle.is_blocked_at(started_at + Duration::from_millis(250)));

        let second_attempt_at = started_at + Duration::from_millis(250);
        assert_eq!(throttle.record_failure_at(second_attempt_at), Duration::from_millis(500));
        assert!(throttle.is_blocked_at(second_attempt_at + Duration::from_millis(499)));

        throttle.reset();
        assert!(!throttle.is_blocked_at(second_attempt_at));
        assert_eq!(throttle.record_failure_at(second_attempt_at), Duration::from_millis(250));
    }

    /// Proves repeated failures cannot cause an impractically long or overflowing cooldown.
    #[test]
    fn caps_the_progressive_cooldown() {
        let mut throttle = UnlockThrottle::new();
        let started_at = Instant::now();

        for attempt in 0..20 {
            let attempt_at = started_at + Duration::from_secs(attempt);
            throttle.record_failure_at(attempt_at);
        }

        assert_eq!(
            throttle.record_failure_at(started_at + Duration::from_secs(20)),
            Duration::from_secs(30)
        );
        assert!(!throttle.is_blocked_at(started_at + Duration::from_secs(20 + 30)));
    }
}
