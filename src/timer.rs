//! WCA-style timer: hold, release to start, press or slam to stop. Timestamps are nanoseconds.

const MS: u64 = 1_000_000;
/// Slams are ignored this long into a solve, to skip the knock of picking up the cube.
const PICKUP_NS: u64 = 1000 * MS;
/// Presses this soon after a stop are part of it. Tapping the screen to stop jolts the phone,
/// so a slam stops the timer just before the touch arrives.
const SETTLE_NS: u64 = 300 * MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timer {
    #[default]
    Idle,
    Holding {
        since: u64,
    },
    Running {
        start: u64,
    },
    /// Idle after a solve that stopped `at`.
    Stopped {
        at: u64,
    },
}

impl Timer {
    /// Returns the solve time in ms if this press stops the timer.
    pub fn press(&mut self, now: u64) -> Option<u64> {
        match *self {
            Timer::Running { start } => {
                *self = Timer::Stopped { at: now };
                Some(now.saturating_sub(start) / MS)
            }
            Timer::Stopped { at } if now.saturating_sub(at) < SETTLE_NS => None,
            Timer::Idle | Timer::Stopped { .. } => {
                *self = Timer::Holding { since: now };
                None
            }
            Timer::Holding { .. } => None,
        }
    }

    /// Starts the timer if it was held for at least `hold_ns`, else cancels the hold.
    pub fn release(&mut self, now: u64, hold_ns: u64) {
        if let Timer::Holding { since } = *self {
            *self = if now.saturating_sub(since) >= hold_ns {
                Timer::Running { start: now }
            } else {
                Timer::Idle
            };
        }
    }

    /// An armed slam stops a solve. Returns the solve time in ms if it does.
    pub fn slam(&mut self, t: u64) -> Option<u64> {
        if self.slam_armed(t) {
            self.press(t)
        } else {
            None
        }
    }

    /// A slam stops the timer once the pickup is over.
    pub fn slam_armed(&self, now: u64) -> bool {
        matches!(*self, Timer::Running { start } if now >= start + PICKUP_NS)
    }

    pub fn idle(&self) -> bool {
        matches!(self, Timer::Idle | Timer::Stopped { .. })
    }

    pub fn ready(&self, now: u64, hold_ns: u64) -> bool {
        matches!(*self, Timer::Holding { since } if now.saturating_sub(since) >= hold_ns)
    }

    pub fn elapsed_ms(&self, now: u64) -> Option<u64> {
        match *self {
            Timer::Running { start } => Some(now.saturating_sub(start) / MS),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOLD_NS: u64 = 300 * MS;

    #[test]
    fn short_hold_cancels() {
        let mut t = Timer::default();
        t.press(0);
        assert!(!t.ready(100 * MS, HOLD_NS));
        t.release(100 * MS, HOLD_NS);
        assert_eq!(t, Timer::Idle);
    }

    #[test]
    fn times_a_solve() {
        let mut t = Timer::default();
        t.press(0);
        assert!(t.ready(HOLD_NS, HOLD_NS));
        t.release(500 * MS, HOLD_NS);
        assert_eq!(t.elapsed_ms(1500 * MS), Some(1000));
        assert_eq!(t.press(12_845 * MS), Some(12_345));
        t.release(13_000 * MS, HOLD_NS);
        assert!(t.idle());
    }

    #[test]
    fn stopping_tap_does_not_start() {
        let mut t = Timer::Running { start: 0 };
        // The tap's jolt arrives before the touch.
        assert_eq!(t.slam(5_000 * MS), Some(5_000));
        t.press(5_020 * MS);
        t.release(9_000 * MS, HOLD_NS);
        assert!(t.idle());
        // A later press starts a new hold.
        t.press(9_500 * MS);
        assert!(matches!(t, Timer::Holding { .. }));
    }

    #[test]
    fn slams() {
        let mut t = Timer::default();
        assert_eq!(t.slam(0), None);
        assert_eq!(t, Timer::Idle);
        t.press(0);
        t.release(500 * MS, HOLD_NS);
        // Picking up the cube.
        assert_eq!(t.slam(900 * MS), None);
        assert_eq!(t.slam(10_500 * MS), Some(10_000));
        assert!(t.idle());
    }
}
