//! WCA-style timer: hold, release to start, press to stop. Timestamps are nanoseconds.

use crate::gesture::Gesture;

const MS: u64 = 1_000_000;
/// Hold time before a release starts the timer.
const HOLD_NS: u64 = 300 * MS;
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

    pub fn release(&mut self, now: u64) {
        if let Timer::Holding { since } = *self {
            *self = if now.saturating_sub(since) >= HOLD_NS {
                Timer::Running { start: now }
            } else {
                Timer::Idle
            };
        }
    }

    /// Covering and uncovering press and release, except mid-solve. An armed slam stops a solve.
    /// Returns the solve time in ms if the gesture stops the timer.
    pub fn gesture(&mut self, gesture: Gesture) -> Option<u64> {
        match gesture {
            Gesture::Cover(t) if self.elapsed_ms(t).is_none() => self.press(t),
            Gesture::Uncover(t) => {
                self.release(t);
                None
            }
            Gesture::Slam { t, steady_since } if self.slam_armed(t, steady_since) => self.press(t),
            _ => None,
        }
    }

    /// A slam stops the timer once the pickup is over, if the phone has been flat and still
    /// on the table since.
    pub fn slam_armed(&self, now: u64, steady_since: u64) -> bool {
        matches!(*self, Timer::Running { start }
            if now >= start + PICKUP_NS && steady_since <= start + PICKUP_NS)
    }

    pub fn idle(&self) -> bool {
        matches!(self, Timer::Idle | Timer::Stopped { .. })
    }

    pub fn ready(&self, now: u64) -> bool {
        matches!(*self, Timer::Holding { since } if now.saturating_sub(since) >= HOLD_NS)
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

    fn slam(t: u64) -> Gesture {
        Gesture::Slam { t, steady_since: 0 }
    }

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
        t.release(13_000 * MS);
        assert!(t.idle());
    }

    #[test]
    fn stopping_tap_does_not_start() {
        let mut t = Timer::Running { start: 0 };
        // The tap's jolt arrives before the touch.
        assert_eq!(t.gesture(slam(5_000 * MS)), Some(5_000));
        t.press(5_020 * MS);
        t.release(9_000 * MS);
        assert!(t.idle());
        // A later press starts a new hold.
        t.press(9_500 * MS);
        assert!(matches!(t, Timer::Holding { .. }));
    }

    #[test]
    fn gestures() {
        let mut t = Timer::default();
        assert_eq!(t.gesture(slam(0)), None);
        t.gesture(Gesture::Cover(0));
        t.gesture(Gesture::Uncover(500 * MS));
        assert_eq!(t, Timer::Running { start: 500 * MS });
        // A hand passing over the sensor mid-solve.
        assert_eq!(t.gesture(Gesture::Cover(800 * MS)), None);
        // Picking up the cube.
        assert_eq!(t.gesture(slam(900 * MS)), None);
        assert_eq!(t.gesture(slam(10_500 * MS)), Some(10_000));
        assert!(t.idle());
    }

    #[test]
    fn slam_needs_a_steady_run() {
        let mut t = Timer::Running { start: 0 };
        // Moved 2 s into the solve.
        let moved = Gesture::Slam {
            t: 5_000 * MS,
            steady_since: 2_000 * MS,
        };
        assert_eq!(t.gesture(moved), None);
        // Jostled during the pickup is fine.
        let jostled = Gesture::Slam {
            t: 5_000 * MS,
            steady_since: 600 * MS,
        };
        assert_eq!(t.gesture(jostled), Some(5_000));
    }
}
