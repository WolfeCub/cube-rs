//! Microphone capture for traces, through AAudio. The app doesn't ask for `RECORD_AUDIO`:
//! grant it in the app's settings or with `adb shell pm grant`.

use crate::sensors::clock;
use crate::trace::Tracer;
use ndk::audio::{
    AudioDirection, AudioFormat, AudioPerformanceMode, AudioStream, AudioStreamBuilder, Clockid,
};
use std::thread;

const RATE: i32 = 48_000;
/// 10 ms.
const CHUNK: usize = 480;
const TIMEOUT_NS: i64 = 100_000_000;

/// Records whenever the tracer is listening.
pub fn start(tracer: Tracer) {
    thread::spawn(move || {
        loop {
            tracer.wait_listening(true);
            if let Err(e) = record(&tracer) {
                log::error!("microphone (is RECORD_AUDIO granted?): {e}");
            }
            tracer.wait_listening(false);
        }
    });
}

/// Records until the tracer stops listening.
fn record(tracer: &Tracer) -> ndk::audio::Result<()> {
    let stream = AudioStreamBuilder::new()?
        .direction(AudioDirection::Input)
        .format(AudioFormat::PCM_I16)
        .channel_count(1)
        .sample_rate(RATE)
        .performance_mode(AudioPerformanceMode::LowLatency)
        .open_stream()?;
    stream.request_start()?;
    let rate = stream.sample_rate() as u32;
    let mut samples = [0i16; CHUNK];
    let mut frame = 0;
    // Timestamps fail until the stream is running; only the first failure is logged.
    let mut failed = false;
    loop {
        let read = unsafe { stream.read(samples.as_mut_ptr().cast(), CHUNK as i32, TIMEOUT_NS) };
        let n = match read {
            Ok(n) => n,
            // ndk 0.9 returns the frame count as an error.
            Err(e) if i32::from(e) > 0 => i32::from(e) as u32,
            Err(e) => return Err(e),
        };
        let captured = match captured(&stream) {
            Ok(captured) => Some(captured),
            Err(e) => {
                if !failed {
                    log::warn!("audio timestamp: {e}");
                    failed = true;
                }
                None
            }
        };
        if !tracer.audio(rate, frame, &samples[..n as usize], captured) {
            return stream.request_stop();
        }
        frame += u64::from(n);
    }
}

/// A frame and when it was captured, on the sensor clock (`CLOCK_BOOTTIME`). Converted from
/// `CLOCK_MONOTONIC`, which AAudio's input streams may only support.
fn captured(stream: &AudioStream) -> ndk::audio::Result<(u64, u64)> {
    let captured = stream.timestamp(Clockid::Monotonic)?;
    let boot = clock(libc::CLOCK_BOOTTIME) - clock(libc::CLOCK_MONOTONIC);
    let t = captured.time_nanoseconds + boot;
    Ok((captured.frame_position as u64, t as u64))
}
