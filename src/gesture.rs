//! Slams from accelerometer readings. Timestamps are nanoseconds.
//!
//! A slam is a sharp vertical knock, like slamming the table next to the phone. It is judged from
//! the jolt itself: a short window after the deviation from gravity jumps, which must be big and
//! mostly vertical. Table bumps are smaller; nudging the phone and its vibration motor are mostly
//! sideways.

/// Deviation from gravity (m/s²) that starts an event. The phone at rest reads ≤ 0.05.
const ONSET: f32 = 0.6;
/// How long after its onset an event is judged. A slam peaks within ~40 ms.
const WINDOW_NS: u64 = 60_000_000;
/// Peak vertical deviation of a slam, by default. Recorded table bumps peak at up to 1.64, slams
/// from 1.73.
pub const SLAM_VERTICAL: f32 = 1.68;
/// A slam's vertical peak is at least this many times its sideways one. Recorded slams: 6 and
/// up; nudges and the vibration motor: 0.5 and below.
const SLAM_RATIO: f32 = 3.0;
/// Time constant of the gravity estimate: it tracks tilting the phone but not a sudden knock.
const GRAVITY_TAU_NS: f32 = 22e6;

#[derive(Debug)]
pub struct Detector {
    /// Peak vertical deviation of a slam.
    pub slam_vertical: f32,
    /// Gravity estimate, and the time of the last reading.
    gravity: Option<([f32; 3], u64)>,
    /// The last reading was above the onset.
    above: bool,
    /// The jolt being judged.
    event: Option<Event>,
}

/// Peak deviations from gravity since `onset`.
#[derive(Debug)]
struct Event {
    onset: u64,
    vertical: f32,
    sideways: f32,
}

impl Default for Detector {
    fn default() -> Self {
        Self {
            slam_vertical: SLAM_VERTICAL,
            gravity: None,
            above: false,
            event: None,
        }
    }
}

impl Detector {
    /// Returns a slam's onset once it has been judged.
    pub fn acceleration(&mut self, t: u64, a: [f32; 3]) -> Option<u64> {
        let (gravity, last) = self.gravity.get_or_insert((a, t));
        let dt = t.saturating_sub(std::mem::replace(last, t));
        let delta: [f32; 3] = std::array::from_fn(|i| a[i] - gravity[i]);
        let up = gravity.map(|g| g / norm(*gravity));
        let vertical: f32 = delta.iter().zip(up).map(|(d, u)| d * u).sum();
        let sideways = norm(std::array::from_fn(|i| delta[i] - vertical * up[i]));
        let vertical = vertical.abs();
        let above = norm(delta) > ONSET;
        let was_above = std::mem::replace(&mut self.above, above);
        // Only a rise starts an event, so a stale gravity estimate catches up between events.
        let rose = above && !was_above;

        let Some(event) = &mut self.event else {
            if rose {
                self.event = Some(Event {
                    onset: t,
                    vertical,
                    sideways,
                });
            } else {
                // Gravity holds still while an event is judged.
                let weight = 1.0 - (-(dt as f32) / GRAVITY_TAU_NS).exp();
                for (g, d) in gravity.iter_mut().zip(delta) {
                    *g += weight * d;
                }
            }
            return None;
        };
        event.vertical = event.vertical.max(vertical);
        event.sideways = event.sideways.max(sideways);
        if t < event.onset + WINDOW_NS {
            return None;
        }
        let Event {
            onset,
            vertical,
            sideways,
        } = self.event.take()?;
        let slam = vertical >= self.slam_vertical && vertical >= SLAM_RATIO * sideways;
        slam.then_some(onset)
    }
}

fn norm(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f32 = 9.81;
    const FLAT: [f32; 3] = [0.0, 0.0, G];
    /// 400 Hz.
    const PERIOD: u64 = 2_500_000;
    /// The knock in `knock`.
    const ONSET_T: u64 = 400 * PERIOD;

    /// The slams from resting at `rest` for a second, a 5 ms knock of `push`, and rest again.
    fn knock(rest: [f32; 3], push: [f32; 3]) -> Vec<u64> {
        let mut d = Detector::default();
        (0..600)
            .filter_map(|i| {
                let noise = if i % 2 == 0 { 0.05 } else { -0.05 };
                let mut a = rest;
                a[0] += noise;
                if (400..402).contains(&i) {
                    a = std::array::from_fn(|j| a[j] + push[j]);
                }
                d.acceleration(i * PERIOD, a)
            })
            .collect()
    }

    #[test]
    fn slam_on_a_table() {
        assert_eq!(knock(FLAT, [0.2, 0.3, 3.0]), [ONSET_T]);
    }

    #[test]
    fn slam_on_a_tilted_phone() {
        let up = [0.0, 0.6, 0.8];
        let rest = up.map(|u| G * u);
        assert_eq!(knock(rest, up.map(|u| 3.0 * u)), [ONSET_T]);
    }

    #[test]
    fn bump_is_not_a_slam() {
        assert_eq!(knock(FLAT, [0.1, 0.1, 1.4]), []);
    }

    #[test]
    fn nudge_is_not_a_slam() {
        assert_eq!(knock(FLAT, [3.0, 1.0, 0.5]), []);
    }

    #[test]
    fn gravity_catches_up_after_a_step() {
        let mut d = Detector::default();
        // The resting reading jumps by more than the onset, then a bump comes 1 s later.
        let slams: Vec<_> = (0..1000)
            .filter_map(|i| {
                let mut z = if i < 400 { G - 0.7 } else { G + 0.15 };
                if (800..802).contains(&i) {
                    z += 1.2;
                }
                d.acceleration(i * PERIOD, [0.0, 0.0, z])
            })
            .collect();
        assert_eq!(slams, []);
    }

    #[test]
    fn tilting_is_not_a_slam() {
        let mut d = Detector::default();
        // Flat to on its side over one second.
        for i in 0..=400 {
            let angle = std::f32::consts::FRAC_PI_2 * i as f32 / 400.0;
            let a = [G * angle.sin(), 0.0, G * angle.cos()];
            assert_eq!(d.acceleration(i * PERIOD, a), None, "sample {i}");
        }
    }
}
