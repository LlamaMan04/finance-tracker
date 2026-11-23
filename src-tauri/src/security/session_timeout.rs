//! In-memory inactivity policy for short-lived authenticated sessions.

use std::time::{Duration, Instant};

/// Keeps an unattended unlocked profile available briefly without retaining it indefinitely.
pub const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Decides whether an authenticated session must be closed after user inactivity.
pub(crate) struct SessionTimeout {
    duration: Duration,
}

impl SessionTimeout {
    /// Creates a policy with an explicit duration so platform configuration can replace the default.
    pub(crate) fn new(duration: Duration) -> Self {
        Self { duration }
    }

    /// Returns the configured duration in whole milliseconds for the non-sensitive frontend timer.
    pub(crate) fn milliseconds(&self) -> u64 {
        u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX)
    }

    /// Reports whether the elapsed idle duration has reached the configured lock threshold.
    pub(crate) fn has_expired_at(&self, last_activity_at: Instant, now: Instant) -> bool {
        now.checked_duration_since(last_activity_at)
            .is_some_and(|idle_duration| idle_duration >= self.duration)
    }
}

#[cfg(test)]
mod tests {
    use super::SessionTimeout;
    use std::time::{Duration, Instant};

    /// Proves an exact timeout boundary locks, while recent activity remains valid.
    #[test]
    fn expires_only_after_the_configured_idle_duration() {
        let policy = SessionTimeout::new(Duration::from_secs(30));
        let activity_at = Instant::now();

        assert!(!policy.has_expired_at(activity_at, activity_at + Duration::from_secs(29)));
        assert!(policy.has_expired_at(activity_at, activity_at + Duration::from_secs(30)));
    }
}
