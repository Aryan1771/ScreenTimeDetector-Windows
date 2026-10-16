use std::time::Duration;

/// Credits one foreground process only while it remains active across samples.
/// Losing focus, minimizing, pausing, locking or sleeping breaks the interval.
#[derive(Default)]
pub struct ActivityClock {
    previous: Option<u32>,
    remainder: Duration,
}

impl ActivityClock {
    pub fn observe(&mut self, elapsed: Duration, foreground: Option<u32>) -> Option<(u32, i64)> {
        let stable = foreground.is_some() && foreground == self.previous;
        self.previous = foreground;
        if !stable || elapsed > Duration::from_secs(3) {
            self.remainder = Duration::ZERO;
            return None;
        }
        self.remainder += elapsed;
        let seconds = self.remainder.as_secs();
        self.remainder -= Duration::from_secs(seconds);
        (seconds > 0).then(|| (foreground.unwrap(), seconds as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switching_minimizing_and_restoring_never_credit_background_apps() {
        let mut clock = ActivityClock::default();
        let mut credited = Vec::new();
        // Two apps are open; only the focused one receives each elapsed second.
        for foreground in [
            Some(1),
            Some(1),
            Some(2),
            Some(2),
            None,
            None,
            Some(1),
            Some(1),
        ] {
            if let Some(usage) = clock.observe(Duration::from_secs(1), foreground) {
                credited.push(usage);
            }
        }
        assert_eq!(credited, [(1, 1), (2, 1), (1, 1)]);
        assert!(credited.iter().map(|(_, seconds)| seconds).sum::<i64>() <= 8);
    }

    #[test]
    fn sleep_pause_idle_and_partial_intervals_do_not_leak_to_another_app() {
        let mut clock = ActivityClock::default();
        let tick = Duration::from_millis(600);
        assert_eq!(clock.observe(tick, Some(1)), None);
        assert_eq!(clock.observe(tick, Some(1)), None);
        assert_eq!(clock.observe(tick, Some(1)), Some((1, 1)));
        assert_eq!(clock.observe(tick, None), None);
        assert_eq!(clock.observe(tick, Some(2)), None);
        assert_eq!(clock.observe(tick, Some(2)), None);
        assert_eq!(clock.observe(Duration::from_secs(600), Some(2)), None);
        assert_eq!(clock.observe(Duration::from_secs(1), Some(2)), Some((2, 1)));
    }
}
