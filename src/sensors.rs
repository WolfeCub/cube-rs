//! Accelerometer and gyroscope through the NDK.

use crate::gesture::Detector;
use crate::trace::{Reading, Tracer};
use ndk::looper::{ForeignLooper, ThreadLooper};
use ndk_sys as ffi;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::{mem, ptr, thread};

pub struct Sensors {
    slams: Receiver<u64>,
    active: Arc<AtomicBool>,
    /// The detector's `slam_vertical`, as f32 bits.
    slam_vertical: Arc<AtomicU32>,
    looper: ForeignLooper,
}

impl Sensors {
    /// Reads the sensors on a background thread, calling `wake` after each slam.
    pub fn start(wake: impl Fn() + Send + 'static, tracer: Tracer) -> Self {
        let (tx, slams) = channel();
        let (looper_tx, looper) = channel();
        let active = Arc::new(AtomicBool::new(false));
        let wanted = active.clone();
        let mut detector = Detector::default();
        let slam_vertical = Arc::new(AtomicU32::new(detector.slam_vertical.to_bits()));
        let threshold = slam_vertical.clone();
        thread::spawn(move || {
            let looper = ThreadLooper::prepare();
            looper_tx.send(looper.as_foreign().clone()).unwrap();
            let queue = Queue::new(&looper);
            let mut enabled = false;
            loop {
                let active = wanted.load(Ordering::Relaxed);
                if active != enabled {
                    log::info!("sensors active: {active}");
                    queue.enable(active);
                    enabled = active;
                }
                looper.poll_once().unwrap();
                detector.slam_vertical = f32::from_bits(threshold.load(Ordering::Relaxed));
                for (t, reading) in queue.readings() {
                    tracer.reading(t, reading);
                    if let Reading::Acceleration(a) = reading
                        && let Some(slam) = detector.acceleration(t, a)
                    {
                        tracer.slam(slam);
                        if tx.send(slam).is_err() {
                            return;
                        }
                        wake();
                    }
                }
            }
        });
        Self {
            slams,
            active,
            slam_vertical,
            looper: looper.recv().unwrap(),
        }
    }

    /// Sensors only run while active, to save battery.
    pub fn set_active(&self, active: bool) {
        if self.active.swap(active, Ordering::Relaxed) != active {
            self.looper.wake();
        }
    }

    /// The slam threshold (peak vertical m/s²), from the next readings on.
    pub fn set_slam_vertical(&self, slam_vertical: f32) {
        self.slam_vertical
            .store(slam_vertical.to_bits(), Ordering::Relaxed);
    }

    /// The next slam's onset, if any.
    pub fn slam(&self) -> Option<u64> {
        self.slams.try_recv().ok()
    }
}

/// A sensor event queue on the current thread's looper. Missing sensors are null and skipped.
/// The raw pointers keep it on that thread.
struct Queue {
    manager: *mut ffi::ASensorManager,
    queue: *mut ffi::ASensorEventQueue,
    accelerometer: *const ffi::ASensor,
    gyroscope: *const ffi::ASensor,
}

impl Queue {
    fn new(looper: &ThreadLooper) -> Self {
        let package = c"io.github.wolfecub.cube_rs".as_ptr();
        // SAFETY: `package` is a C string.
        let manager = unsafe { ffi::ASensorManager_getInstanceForPackage(package) };
        assert!(!manager.is_null(), "no sensor manager");
        let looper = looper.as_foreign().ptr().as_ptr();
        // SAFETY: `manager` and `looper` are valid, and the null callback and data are allowed.
        let queue = unsafe {
            ffi::ASensorManager_createEventQueue(manager, looper, 0, None, ptr::null_mut())
        };
        assert!(!queue.is_null(), "no sensor event queue");
        // SAFETY: `manager` is valid.
        let sensor = |kind| unsafe { ffi::ASensorManager_getDefaultSensor(manager, kind) };
        Self {
            manager,
            queue,
            accelerometer: sensor(ffi::ASENSOR_TYPE_ACCELEROMETER),
            gyroscope: sensor(ffi::ASENSOR_TYPE_GYROSCOPE),
        }
    }

    /// Each sensor at its fastest rate. Above 200 Hz needs `HIGH_SAMPLING_RATE_SENSORS`.
    fn enable(&self, on: bool) {
        for sensor in [self.accelerometer, self.gyroscope] {
            if sensor.is_null() {
                continue;
            }
            // SAFETY: `queue` and `sensor` are valid and non-null.
            if on {
                let period = unsafe { ffi::ASensor_getMinDelay(sensor) };
                let result =
                    unsafe { ffi::ASensorEventQueue_registerSensor(self.queue, sensor, period, 0) };
                let kind = unsafe { ffi::ASensor_getType(sensor) };
                log::info!("sensor {kind} every {period} µs: {result}");
            } else {
                unsafe { ffi::ASensorEventQueue_disableSensor(self.queue, sensor) };
            }
        }
    }

    /// Drains the queue.
    fn readings(&self) -> Vec<(u64, Reading)> {
        let mut readings = Vec::new();
        // SAFETY: all zeros is a valid `ASensorEvent`, a plain C struct.
        let mut events: [ffi::ASensorEvent; 32] = unsafe { mem::zeroed() };
        loop {
            // SAFETY: `queue` is valid, and `events` holds `events.len()` events.
            let n = unsafe {
                ffi::ASensorEventQueue_getEvents(self.queue, events.as_mut_ptr(), events.len())
            };
            if n <= 0 {
                return readings;
            }
            for event in &events[..n as usize] {
                // SAFETY: any bits are valid f32s.
                let data = unsafe { event.__bindgen_anon_1.__bindgen_anon_1.data };
                let xyz = [data[0], data[1], data[2]];
                let reading = match event.type_ {
                    ffi::ASENSOR_TYPE_ACCELEROMETER => Reading::Acceleration(xyz),
                    ffi::ASENSOR_TYPE_GYROSCOPE => Reading::Rotation(xyz),
                    _ => continue,
                };
                readings.push((event.timestamp as u64, reading));
            }
        }
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        // SAFETY: `queue` came from `manager` and isn't used again.
        unsafe { ffi::ASensorManager_destroyEventQueue(self.manager, self.queue) };
    }
}
