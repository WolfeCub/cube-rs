//! Proximity sensor and accelerometer through the NDK.

use crate::gesture::{Detector, Gesture};
use ndk::looper::{ForeignLooper, ThreadLooper};
use ndk_sys as ffi;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::{mem, ptr, thread};

/// 200 Hz, the fastest rate without the high sampling rate permission.
const ACCELEROMETER_PERIOD_US: i32 = 5_000;

/// Nanoseconds on the sensor event clock.
pub fn now() -> u64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

pub struct Sensors {
    gestures: Receiver<Gesture>,
    active: Arc<AtomicBool>,
    steady_since: Arc<AtomicU64>,
    looper: ForeignLooper,
}

impl Sensors {
    /// Reads the sensors on a background thread, calling `wake` after each gesture.
    pub fn start(wake: impl Fn() + Send + 'static) -> Self {
        let (tx, gestures) = channel();
        let (looper_tx, looper) = channel();
        let active = Arc::new(AtomicBool::new(false));
        let wanted = active.clone();
        let steady_since = Arc::new(AtomicU64::new(0));
        let steady = steady_since.clone();
        thread::spawn(move || {
            let looper = ThreadLooper::prepare();
            looper_tx.send(looper.as_foreign().clone()).unwrap();
            let queue = Queue::new(&looper);
            let mut enabled = false;
            let mut detector = Detector::default();
            loop {
                let active = wanted.load(Ordering::Relaxed);
                if active != enabled {
                    log::info!("sensors active: {active}");
                    queue.enable(active);
                    enabled = active;
                }
                looper.poll_once().unwrap();
                for (t, reading) in queue.readings() {
                    let gesture = match reading {
                        Reading::Distance(d) => detector.proximity(t, d),
                        Reading::Acceleration(a) => {
                            let slam = detector.acceleration(t, a);
                            steady.store(detector.steady_since(), Ordering::Relaxed);
                            slam
                        }
                    };
                    if let Some(gesture) = gesture {
                        if tx.send(gesture).is_err() {
                            return;
                        }
                        wake();
                    }
                }
            }
        });
        Self {
            gestures,
            active,
            steady_since,
            looper: looper.recv().unwrap(),
        }
    }

    /// Sensors only run while active, to save battery.
    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, Ordering::Relaxed) != active {
            self.looper.wake();
        }
    }

    /// The phone has been flat and still since then.
    pub fn steady_since(&self) -> u64 {
        self.steady_since.load(Ordering::Relaxed)
    }

    /// The next gesture, if any.
    pub fn gesture(&self) -> Option<Gesture> {
        self.gestures.try_recv().ok()
    }
}

enum Reading {
    /// Centimetres.
    Distance(f32),
    /// m/s², including gravity.
    Acceleration([f32; 3]),
}

/// A sensor event queue on the current thread's looper. Missing sensors are null and skipped.
struct Queue {
    queue: *mut ffi::ASensorEventQueue,
    proximity: *const ffi::ASensor,
    accelerometer: *const ffi::ASensor,
}

impl Queue {
    fn new(looper: &ThreadLooper) -> Self {
        unsafe {
            let manager =
                ffi::ASensorManager_getInstanceForPackage(c"io.github.wolfecub.cube_rs".as_ptr());
            let looper = looper.as_foreign().ptr().as_ptr();
            let sensor = |kind| ffi::ASensorManager_getDefaultSensor(manager, kind);
            Self {
                queue: ffi::ASensorManager_createEventQueue(
                    manager,
                    looper,
                    0,
                    None,
                    ptr::null_mut(),
                ),
                proximity: sensor(ffi::ASENSOR_TYPE_PROXIMITY),
                accelerometer: sensor(ffi::ASENSOR_TYPE_ACCELEROMETER),
            }
        }
    }

    fn enable(&self, on: bool) {
        for (sensor, period) in [
            (self.proximity, 0),
            (self.accelerometer, ACCELEROMETER_PERIOD_US),
        ] {
            if sensor.is_null() {
                continue;
            }
            unsafe {
                if on {
                    let period = period.max(ffi::ASensor_getMinDelay(sensor));
                    ffi::ASensorEventQueue_registerSensor(self.queue, sensor, period, 0);
                } else {
                    ffi::ASensorEventQueue_disableSensor(self.queue, sensor);
                }
            }
        }
    }

    /// Drains the queue.
    fn readings(&self) -> Vec<(u64, Reading)> {
        let mut readings = Vec::new();
        let mut events: [ffi::ASensorEvent; 32] = unsafe { mem::zeroed() };
        loop {
            let n = unsafe {
                ffi::ASensorEventQueue_getEvents(self.queue, events.as_mut_ptr(), events.len())
            };
            if n <= 0 {
                return readings;
            }
            for event in &events[..n as usize] {
                let data = unsafe { event.__bindgen_anon_1.__bindgen_anon_1.data };
                let reading = match event.type_ {
                    ffi::ASENSOR_TYPE_PROXIMITY => Reading::Distance(data[0]),
                    ffi::ASENSOR_TYPE_ACCELEROMETER => {
                        Reading::Acceleration([data[0], data[1], data[2]])
                    }
                    _ => continue,
                };
                readings.push((event.timestamp as u64, reading));
            }
        }
    }
}
