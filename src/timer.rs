//! WCA-style timer: tap to inspect if inspecting, hold, release to start, press or slam to stop.
//! Timestamps are nanoseconds.

use crate::session::Penalty;

const MS: u64 = 1_000_000;
/// WCA inspection: starting within 15 s is fine, within 17 s is +2, later is DNF.
pub const INSPECTION_NS: u64 = 15_000 * MS;
const INSPECTION_PLUS_TWO_NS: u64 = 17_000 * MS;
/// When a WCA judge calls out the inspection time.
const INSPECTION_CALLS_NS: [u64; 2] = [8_000 * MS, 12_000 * MS];
/// Slams are ignored this long into a solve, to skip the knock of picking up the cube.
const PICKUP_NS: u64 = 1000 * MS;
/// Presses this soon after a stop are part of it. Tapping the screen to stop jolts the phone,
/// so a slam stops the timer just before the touch arrives.
const SETTLE_NS: u64 = 300 * MS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Timer {
    #[default]
    Idle,
    /// Inspecting since `since`.
    Inspecting { since: u64 },
    /// Held since `since`, after inspecting since `inspected` if inspecting.
    Holding { since: u64, inspected: Option<u64> },
    /// Running with `penalty` from inspection.
    Running { start: u64, penalty: Penalty },
    /// Idle after a solve that stopped `at`.
    Stopped { at: u64 },
}

impl Timer {
    /// Returns the solve time in ms and its penalty if this press stops the timer. With
    /// `inspection`, a press from idle starts inspecting rather than a hold.
    pub fn press(&mut self, now: u64, inspection: bool) -> Option<(u64, Penalty)> {
        match *self {
            Timer::Running { .. } => self.stop(now),
            Timer::Stopped { at } if now.saturating_sub(at) < SETTLE_NS => None,
            Timer::Idle | Timer::Stopped { .. } if inspection => {
                *self = Timer::Inspecting { since: now };
                None
            }
            Timer::Idle | Timer::Stopped { .. } => {
                *self = Timer::Holding {
                    since: now,
                    inspected: None,
                };
                None
            }
            Timer::Inspecting { since } => {
                *self = Timer::Holding {
                    since: now,
                    inspected: Some(since),
                };
                None
            }
            Timer::Holding { .. } => None,
        }
    }

    /// Starts the timer if it was held for at least `hold_ns`, else goes back to inspecting or
    /// idle.
    pub fn release(&mut self, now: u64, hold_ns: u64) {
        if let Timer::Holding { since, inspected } = *self {
            *self = match inspected {
                _ if now.saturating_sub(since) >= hold_ns => Timer::Running {
                    start: now,
                    penalty: inspected.map_or(Penalty::None, |at| inspection_penalty(now - at)),
                },
                Some(since) => Timer::Inspecting { since },
                None => Timer::Idle,
            };
        }
    }

    /// An armed slam stops a solve. Returns the solve time in ms and its penalty if it does.
    pub fn slam(&mut self, t: u64) -> Option<(u64, Penalty)> {
        if self.slam_armed(t) {
            self.stop(t)
        } else {
            None
        }
    }

    fn stop(&mut self, now: u64) -> Option<(u64, Penalty)> {
        let Timer::Running { start, penalty } = *self else {
            return None;
        };
        *self = Timer::Stopped { at: now };
        Some((now.saturating_sub(start) / MS, penalty))
    }

    /// Ends an inspection without a solve.
    pub fn cancel_inspection(&mut self) {
        if let Timer::Inspecting { .. } = self {
            *self = Timer::Idle;
        }
    }

    /// A slam stops the timer once the pickup is over.
    pub fn slam_armed(&self, now: u64) -> bool {
        matches!(*self, Timer::Running { start, .. } if now >= start + PICKUP_NS)
    }

    /// How long inspection has gone, while inspecting or holding after it.
    pub fn inspected_ns(&self, now: u64) -> Option<u64> {
        match *self {
            Timer::Inspecting { since }
            | Timer::Holding {
                inspected: Some(since),
                ..
            } => Some(now.saturating_sub(since)),
            _ => None,
        }
    }

    pub fn idle(&self) -> bool {
        matches!(self, Timer::Idle | Timer::Stopped { .. })
    }

    pub fn ready(&self, now: u64, hold_ns: u64) -> bool {
        matches!(*self, Timer::Holding { since, .. } if now.saturating_sub(since) >= hold_ns)
    }

    pub fn elapsed_ms(&self, now: u64) -> Option<u64> {
        match *self {
            Timer::Running { start, .. } => Some(now.saturating_sub(start) / MS),
            _ => None,
        }
    }
}

/// How many inspection calls are due after inspecting for `ns`: 1 at 8 s, 2 at 12 s.
pub fn inspection_calls(ns: u64) -> usize {
    INSPECTION_CALLS_NS.iter().filter(|&&at| ns >= at).count()
}

/// The penalty for starting a solve after inspecting for `ns`.
pub fn inspection_penalty(ns: u64) -> Penalty {
    if ns <= INSPECTION_NS {
        Penalty::None
    } else if ns <= INSPECTION_PLUS_TWO_NS {
        Penalty::PlusTwo
    } else {
        Penalty::Dnf
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOLD_NS: u64 = 300 * MS;

    #[test]
    fn short_hold_cancels() {
        let mut t = Timer::default();
        t.press(0, false);
        assert!(!t.ready(100 * MS, HOLD_NS));
        t.release(100 * MS, HOLD_NS);
        assert_eq!(t, Timer::Idle);
    }

    #[test]
    fn times_a_solve() {
        let mut t = Timer::default();
        t.press(0, false);
        assert!(t.ready(HOLD_NS, HOLD_NS));
        t.release(500 * MS, HOLD_NS);
        assert_eq!(t.elapsed_ms(1500 * MS), Some(1000));
        assert_eq!(t.press(12_845 * MS, false), Some((12_345, Penalty::None)));
        t.release(13_000 * MS, HOLD_NS);
        assert!(t.idle());
    }

    #[test]
    fn stopping_tap_does_not_start() {
        let mut t = Timer::Running {
            start: 0,
            penalty: Penalty::None,
        };
        // The tap's jolt arrives before the touch.
        assert_eq!(t.slam(5_000 * MS), Some((5_000, Penalty::None)));
        t.press(5_020 * MS, false);
        t.release(9_000 * MS, HOLD_NS);
        assert!(t.idle());
        // A later press starts a new hold.
        t.press(9_500 * MS, false);
        assert!(matches!(t, Timer::Holding { .. }));
    }

    #[test]
    fn slams() {
        let mut t = Timer::default();
        assert_eq!(t.slam(0), None);
        assert_eq!(t, Timer::Idle);
        t.press(0, false);
        t.release(500 * MS, HOLD_NS);
        // Picking up the cube.
        assert_eq!(t.slam(900 * MS), None);
        assert_eq!(t.slam(10_500 * MS), Some((10_000, Penalty::None)));
        assert!(t.idle());
    }

    /// Inspects from 0, then holds from `hold` and releases 500 ms later. Returns the penalty.
    fn inspect(hold: u64) -> Penalty {
        let mut t = Timer::default();
        t.press(0, true);
        t.release(100 * MS, HOLD_NS);
        assert_eq!(t, Timer::Inspecting { since: 0 });
        t.press(hold, true);
        t.release(hold + 500 * MS, HOLD_NS);
        let stop = t.press(hold + 10_500 * MS, true);
        stop.unwrap().1
    }

    #[test]
    fn inspection_penalties() {
        assert_eq!(inspect(14_000 * MS), Penalty::None);
        assert_eq!(inspect(16_000 * MS), Penalty::PlusTwo);
        assert_eq!(inspect(17_000 * MS), Penalty::Dnf);
    }

    #[test]
    fn inspection_calls_at_8_and_12_s() {
        let calls = [0, 7_999, 8_000, 11_999, 12_000, 20_000].map(|ms| inspection_calls(ms * MS));
        assert_eq!(calls, [0, 0, 1, 1, 2, 2]);
    }

    #[test]
    fn short_hold_keeps_inspecting() {
        let mut t = Timer::Inspecting { since: 0 };
        t.press(5_000 * MS, true);
        assert_eq!(t.inspected_ns(5_100 * MS), Some(5_100 * MS));
        t.release(5_100 * MS, HOLD_NS);
        assert_eq!(t, Timer::Inspecting { since: 0 });
        t.cancel_inspection();
        assert_eq!(t, Timer::Idle);
    }
}
