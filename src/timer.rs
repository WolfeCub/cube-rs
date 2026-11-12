//! WCA-style timer: hold, release to start, press to stop. Timestamps are nanoseconds.

const MS: u64 = 1_000_000;
/// Hold time before a release starts the timer.
pub const HOLD_NS: u64 = 300 * MS;

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
}

impl Timer {
    /// Returns the solve time in ms if this press stops the timer.
    pub fn press(&mut self, now: u64) -> Option<u64> {
        match *self {
            Timer::Idle => {
                *self = Timer::Holding { since: now };
                None
            }
            Timer::Holding { .. } => None,
            Timer::Running { start } => {
                *self = Timer::Idle;
                Some((now - start) / MS)
            }
        }
    }

    pub fn release(&mut self, now: u64) {
        if let Timer::Holding { since } = *self {
            *self = if now - since >= HOLD_NS {
                Timer::Running { start: now }
            } else {
                Timer::Idle
            };
        }
    }

    pub fn ready(&self, now: u64) -> bool {
        matches!(*self, Timer::Holding { since } if now - since >= HOLD_NS)
    }

    pub fn elapsed_ms(&self, now: u64) -> Option<u64> {
        match *self {
            Timer::Running { start } => Some((now - start) / MS),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_hold_cancels() {
        let mut t = Timer::default();
        t.press(0);
        assert!(!t.ready(100 * MS));
        t.release(100 * MS);
        assert_eq!(t, Timer::Idle);
    }

    #[test]
    fn times_a_solve() {
        let mut t = Timer::default();
        t.press(0);
        assert!(t.ready(HOLD_NS));
        t.release(500 * MS);
        assert_eq!(t.elapsed_ms(1500 * MS), Some(1000));
        assert_eq!(t.press(12_845 * MS), Some(12_345));
        // Lifting the stopping finger does nothing.
        t.release(13_000 * MS);
        assert_eq!(t, Timer::Idle);
    }
}
