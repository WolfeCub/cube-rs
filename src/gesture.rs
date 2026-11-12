//! Gestures from sensor readings. Timestamps are nanoseconds.

/// Deviation from gravity (m/s²) that counts as a slam. Measured on a table: rest 0.05,
/// gentle slam 0.65, hard slam 13.
const SLAM_THRESHOLD: f32 = 0.6;
/// Deviation that counts as the phone moving. A slam jumps past both thresholds at once;
/// handling the phone builds up and crosses this one first.
const MOVING_THRESHOLD: f32 = 0.3;
/// Lying face up, within about 20° of flat.
const FLAT_COS: f32 = 0.94;
/// Weight of each accelerometer sample in the gravity estimate. At 200 Hz this tracks
/// tilting the phone but not a sudden knock.
const GRAVITY_SMOOTHING: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// The proximity sensor was covered.
    Cover(u64),
    Uncover(u64),
    /// A sharp knock, like slamming the table next to the phone.
    Slam {
        t: u64,
        /// The phone has been flat and still since then.
        steady_since: u64,
    },
}

#[derive(Debug, Default)]
pub struct Detector {
    /// Largest proximity distance seen. Most sensors only report 0 and their maximum range.
    far: f32,
    near: bool,
    gravity: Option<[f32; 3]>,
    steady_since: u64,
}

impl Detector {
    pub fn proximity(&mut self, t: u64, distance: f32) -> Option<Gesture> {
        self.far = self.far.max(distance);
        let near = distance < self.far;
        let changed = near != std::mem::replace(&mut self.near, near);
        changed.then_some(if near {
            Gesture::Cover(t)
        } else {
            Gesture::Uncover(t)
        })
    }

    pub fn acceleration(&mut self, t: u64, a: [f32; 3]) -> Option<Gesture> {
        let gravity = self.gravity.get_or_insert(a);
        let flat = gravity[2] > FLAT_COS * norm(*gravity);
        let delta: [f32; 3] = std::array::from_fn(|i| a[i] - gravity[i]);
        for (g, d) in gravity.iter_mut().zip(delta) {
            *g += GRAVITY_SMOOTHING * d;
        }
        let deviation = norm(delta);
        let slam = (deviation > SLAM_THRESHOLD).then_some(Gesture::Slam {
            t,
            steady_since: self.steady_since,
        });
        if !flat || deviation > MOVING_THRESHOLD {
            self.steady_since = t;
        }
        slam
    }

    /// The phone has been flat and still since then.
    pub fn steady_since(&self) -> u64 {
        self.steady_since
    }
}

fn norm(v: [f32; 3]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: f32 = 9.81;
    /// 200 Hz.
    const PERIOD: u64 = 5_000_000;

    #[test]
    fn cover_and_uncover() {
        let mut d = Detector::default();
        assert_eq!(d.proximity(1, 5.0), None);
        assert_eq!(d.proximity(2, 0.0), Some(Gesture::Cover(2)));
        assert_eq!(d.proximity(3, 0.0), None);
        assert_eq!(d.proximity(4, 5.0), Some(Gesture::Uncover(4)));
    }

    /// Feeds `a` for one second and returns the gesture from a knock on top of it.
    fn knock(a: [f32; 3]) -> Option<Gesture> {
        let mut d = Detector::default();
        for i in 0..200 {
            let noise = if i % 2 == 0 { 0.05 } else { -0.05 };
            assert_eq!(d.acceleration(i * PERIOD, [a[0] + noise, a[1], a[2]]), None);
        }
        d.acceleration(200 * PERIOD, [a[0] + 1.0, a[1] + 2.0, a[2] + 6.0])
    }

    #[test]
    fn slam_on_a_table() {
        let slam = knock([0.0, 0.0, G]);
        assert_eq!(
            slam,
            Some(Gesture::Slam {
                t: 200 * PERIOD,
                steady_since: 0
            })
        );
    }

    #[test]
    fn slam_while_tilted_is_unsteady() {
        let slam = knock([0.0, G * 0.707, G * 0.707]);
        assert_eq!(
            slam,
            Some(Gesture::Slam {
                t: 200 * PERIOD,
                steady_since: 199 * PERIOD
            })
        );
    }

    #[test]
    fn handling_is_unsteady_before_it_looks_like_a_slam() {
        let mut d = Detector::default();
        d.acceleration(0, [0.0, 0.0, G]);
        // Pushing the phone along the table, building up over a few samples.
        let mut steady_since = 0;
        for (i, push) in [0.2, 0.4, 0.8, 1.2].into_iter().enumerate() {
            let t = (i as u64 + 1) * PERIOD;
            if let Some(Gesture::Slam {
                steady_since: s, ..
            }) = d.acceleration(t, [push, 0.0, G])
            {
                steady_since = s;
            }
        }
        assert_ne!(steady_since, 0);
    }

    #[test]
    fn tilting_is_not_a_slam() {
        let mut d = Detector::default();
        // Flat to on its side over one second.
        for i in 0..=200 {
            let angle = std::f32::consts::FRAC_PI_2 * i as f32 / 200.0;
            let a = [G * angle.sin(), 0.0, G * angle.cos()];
            assert_eq!(d.acceleration(i * PERIOD, a), None, "sample {i}");
        }
    }
}
