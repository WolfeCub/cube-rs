//! Sensor and microphone traces around each solve, saved for tuning gestures offline.
//! Timestamps are nanoseconds on the sensor clock.
//!
//! Each solve gives `<unix seconds>-<solve ms>.csv` and, if the microphone ran, a 16-bit mono
//! `.wav`. The CSV has one record per line: `t,a,x,y,z` (acceleration), `t,g,x,y,z` (rotation),
//! `t,f,frame` (the WAV frame captured at `t`) and `t,<mark>` (`hold`, `start`, `stop`, `slam`).
//! Older traces also have `t,p,distance` (proximity), which `parse` skips.

use crate::timer::Timer;
use std::collections::VecDeque;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::SystemTime;
use std::{fs, mem};

const MS: u64 = 1_000_000;
/// History kept in memory.
const KEEP_NS: u64 = 120_000 * MS;
/// A solve's trace starts this long before it and ends this long after.
const LEAD_NS: u64 = 2_000 * MS;
const TAIL_NS: u64 = 1_000 * MS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reading {
    /// m/s², including gravity.
    Acceleration([f32; 3]),
    /// rad/s.
    Rotation([f32; 3]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Hold,
    Start,
    Stop,
    /// The detector reported a slam, armed or not.
    Slam,
}

const MARKS: [(Mark, &str); 4] = [
    (Mark::Hold, "hold"),
    (Mark::Start, "start"),
    (Mark::Stop, "stop"),
    (Mark::Slam, "slam"),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Record {
    Reading(Reading),
    Mark(Mark),
    /// The WAV frame captured at this time.
    Audio(u64),
}

/// Keeps recent readings and saves them around each solve. Clones share one trace.
#[derive(Debug, Clone)]
pub struct Tracer(Arc<Shared>);

#[derive(Debug)]
struct Shared {
    trace: Mutex<Trace>,
    /// Notified when `Trace::listening` may have changed.
    listening: Condvar,
}

#[derive(Debug, Default)]
struct Trace {
    dir: PathBuf,
    /// Solves are saved, and the microphone runs for them.
    enabled: bool,
    records: VecDeque<(u64, Record)>,
    timer: Timer,
    /// The last solve, saved once its tail is recorded.
    unsaved: Option<Unsaved>,
    /// The last solve's files, without the extension.
    last: Option<PathBuf>,
    /// The microphone runs from a hold until its solve is saved.
    listening: bool,
    audio: Audio,
}

#[derive(Debug)]
struct Unsaved {
    from: u64,
    to: u64,
    /// Without the extension.
    path: PathBuf,
}

/// Microphone samples since the hold.
#[derive(Debug, Default)]
struct Audio {
    rate: u32,
    /// The microphone's frame number of the first sample.
    first: Option<u64>,
    samples: Vec<i16>,
}

/// A solve's files, ready to write.
struct Save {
    path: PathBuf,
    csv: String,
    wav: Option<Vec<u8>>,
}

impl Tracer {
    /// Saves a trace of each solve in `dir`.
    pub fn new(dir: PathBuf) -> Self {
        let trace = Trace {
            dir,
            enabled: true,
            ..Default::default()
        };
        Self(Arc::new(Shared {
            trace: Mutex::new(trace),
            listening: Condvar::new(),
        }))
    }

    /// Turns saving on or off. A solve already waiting for its tail is still saved.
    pub fn set_enabled(&self, enabled: bool) {
        let mut trace = self.lock();
        trace.enabled = enabled;
        if !enabled {
            // So the questionnaire can't answer for an earlier solve.
            trace.last = None;
        }
    }

    pub fn reading(&self, t: u64, reading: Reading) {
        let mut trace = self.lock();
        trace.push(t, Record::Reading(reading));
        if trace.unsaved.as_ref().is_some_and(|u| t >= u.to) {
            let save = trace.finish();
            self.unlock(trace, save);
        }
    }

    pub fn slam(&self, t: u64) {
        self.lock().push(t, Record::Mark(Mark::Slam));
    }

    /// Marks holds, starts and stops. Call whenever the timer may have changed.
    pub fn timer(&self, timer: Timer) {
        let mut trace = self.lock();
        let before = mem::replace(&mut trace.timer, timer);
        let mut save = None;
        match (before, timer) {
            _ if before == timer => return,
            (_, Timer::Holding { since, .. }) => {
                // Cuts the last solve's tail short.
                save = trace.finish();
                trace.push(since, Record::Mark(Mark::Hold));
                trace.listen(true);
            }
            (Timer::Holding { .. }, Timer::Idle | Timer::Inspecting { .. }) => trace.listen(false),
            (_, Timer::Running { start, .. }) => trace.push(start, Record::Mark(Mark::Start)),
            (Timer::Running { start, .. }, Timer::Stopped { at }) => {
                trace.push(at, Record::Mark(Mark::Stop));
                if trace.enabled {
                    let secs = SystemTime::UNIX_EPOCH.elapsed().unwrap_or_default();
                    let ms = (at - start) / MS;
                    let path = trace.dir.join(format!("{}-{ms}", secs.as_secs()));
                    trace.last = Some(path.clone());
                    trace.unsaved = Some(Unsaved {
                        from: start.saturating_sub(LEAD_NS),
                        to: at + TAIL_NS,
                        path,
                    });
                }
            }
            _ => {}
        }
        self.unlock(trace, save);
    }

    /// Saves how the last solve stopped, from the questionnaire, as `<trace>.txt`.
    pub fn answer(&self, answer: &str) {
        let Some(path) = self.lock().last.as_ref().map(|p| p.with_extension("txt")) else {
            return;
        };
        let written =
            fs::create_dir_all(path.parent().unwrap()).and_then(|()| fs::write(&path, answer));
        if let Err(e) = written {
            log::error!("saving {}: {e}", path.display());
        }
    }

    /// Blocks until the microphone should be on, or off.
    pub fn wait_listening(&self, on: bool) {
        let trace = self.lock();
        drop(self.0.listening.wait_while(trace, |t| t.listening != on));
    }

    /// Adds microphone samples, the first being the microphone's frame `first`. `captured` is a
    /// frame and the time it was captured. Returns whether to keep listening.
    pub fn audio(
        &self,
        rate: u32,
        first: u64,
        samples: &[i16],
        captured: Option<(u64, u64)>,
    ) -> bool {
        let mut trace = self.lock();
        if !trace.listening {
            return false;
        }
        let audio = &mut trace.audio;
        audio.rate = rate;
        let start = *audio.first.get_or_insert(first);
        audio.samples.extend_from_slice(samples);
        if let Some((frame, t)) = captured
            && frame >= start
        {
            trace.push(t, Record::Audio(frame - start));
        }
        true
    }

    fn lock(&self) -> MutexGuard<'_, Trace> {
        self.0.trace.lock().unwrap()
    }

    /// Unlocks the trace, wakes the microphone and writes any finished solve.
    fn unlock(&self, trace: MutexGuard<'_, Trace>, save: Option<Save>) {
        drop(trace);
        self.0.listening.notify_all();
        if let Some(save) = save {
            save.write();
        }
    }
}

impl Trace {
    fn push(&mut self, t: u64, record: Record) {
        self.records.push_back((t, record));
        while let Some(&(t0, _)) = self.records.front()
            && t0 + KEEP_NS < t
        {
            self.records.pop_front();
        }
    }

    /// Starts or stops listening, dropping any audio and its timestamps. Their frames count from
    /// the start of that audio, so they would be wrong for the next.
    fn listen(&mut self, on: bool) {
        self.listening = on && self.enabled;
        self.audio = Audio::default();
        self.records.retain(|(_, r)| !matches!(r, Record::Audio(_)));
    }

    /// The last solve's files, if it has any. Stops listening.
    fn finish(&mut self) -> Option<Save> {
        let Unsaved { from, to, path } = self.unsaved.take()?;
        let csv = self.csv(from, to);
        let audio = &self.audio;
        let wav = (!audio.samples.is_empty()).then(|| wav(audio.rate, &audio.samples));
        self.listen(false);
        Some(Save { path, csv, wav })
    }

    /// Records from `from` to `to` in time order. Marks from the UI can arrive out of order.
    fn csv(&self, from: u64, to: u64) -> String {
        let mut records: Vec<_> = self
            .records
            .iter()
            .filter(|(t, _)| (from..=to).contains(t))
            .collect();
        records.sort_by_key(|(t, _)| *t);
        let mut csv = String::new();
        for (t, record) in records {
            let _ = match record {
                Record::Reading(Reading::Acceleration([x, y, z])) => {
                    writeln!(csv, "{t},a,{x},{y},{z}")
                }
                Record::Reading(Reading::Rotation([x, y, z])) => writeln!(csv, "{t},g,{x},{y},{z}"),
                Record::Audio(frame) => writeln!(csv, "{t},f,{frame}"),
                Record::Mark(m) => {
                    let name = MARKS.iter().find(|(n, _)| n == m).unwrap().1;
                    writeln!(csv, "{t},{name}")
                }
            };
        }
        csv
    }
}

impl Save {
    fn write(self) {
        let csv = self.path.with_extension("csv");
        let mut written =
            fs::create_dir_all(csv.parent().unwrap()).and_then(|()| fs::write(&csv, self.csv));
        if let Some(wav) = self.wav {
            written = written.and_then(|()| fs::write(self.path.with_extension("wav"), wav));
        }
        match written {
            Ok(()) => log::info!("saved {}", csv.display()),
            Err(e) => log::error!("saving {}: {e}", csv.display()),
        }
    }
}

/// 16-bit mono WAV.
fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data = 2 * samples.len() as u32;
    let mut wav = Vec::with_capacity(44 + data as usize);
    for field in [
        b"RIFF".as_slice(),
        &(36 + data).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(), // PCM
        &1u16.to_le_bytes(), // mono
        &rate.to_le_bytes(),
        &(2 * rate).to_le_bytes(),
        &2u16.to_le_bytes(),
        &16u16.to_le_bytes(),
        b"data",
        &data.to_le_bytes(),
    ] {
        wav.extend_from_slice(field);
    }
    wav.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    wav
}

/// Parses a saved CSV, skipping lines it doesn't understand.
pub fn parse(csv: &str) -> Vec<(u64, Record)> {
    csv.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(u64, Record)> {
    let mut fields = line.split(',');
    let t = fields.next()?.parse().ok()?;
    let kind = fields.next()?;
    let values: Vec<&str> = fields.collect();
    let f = |i: usize| values.get(i)?.parse().ok();
    let xyz = || Some([f(0)?, f(1)?, f(2)?]);
    let record = match kind {
        "a" => Record::Reading(Reading::Acceleration(xyz()?)),
        "g" => Record::Reading(Reading::Rotation(xyz()?)),
        "f" => Record::Audio(values.first()?.parse().ok()?),
        _ => Record::Mark(MARKS.iter().find(|(_, n)| *n == kind)?.0),
    };
    Some((t, record))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Penalty;

    const DOWN: Reading = Reading::Acceleration([0.0, 0.0, 9.81]);

    #[test]
    fn saves_each_solve() {
        let dir = crate::test_path("trace");
        let tracer = Tracer::new(dir.clone());
        // Before the trace's lead.
        tracer.reading(0, DOWN);
        assert!(
            !tracer.audio(48_000, 0, &[1], None),
            "listening before a hold"
        );
        tracer.timer(Timer::Holding {
            since: 3_000 * MS,
            inspected: None,
        });
        assert!(tracer.audio(48_000, 100, &[1, -2], Some((100, 3_200 * MS))));
        tracer.timer(Timer::Running {
            start: 3_500 * MS,
            penalty: Penalty::None,
        });
        tracer.reading(4_000 * MS, Reading::Rotation([0.0, 0.5, -1.25]));
        tracer.slam(7_000 * MS);
        tracer.timer(Timer::Stopped { at: 7_000 * MS });
        // Repeated states mark nothing.
        tracer.timer(Timer::Stopped { at: 7_000 * MS });
        tracer.reading(7_500 * MS, DOWN);
        assert!(!dir.exists(), "saved before the tail");
        tracer.reading(8_000 * MS, DOWN);
        assert!(
            !tracer.audio(48_000, 102, &[3], None),
            "listening after saving"
        );
        tracer.answer("bump");

        let mut files: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|f| f.unwrap().path())
            .collect();
        files.sort();
        let names: Vec<_> = files.iter().map(|f| f.to_str().unwrap()).collect();
        let [csv, txt, wav] = names[..] else {
            panic!("{names:?}")
        };
        assert!(csv.ends_with("-3500.csv") && txt.ends_with("-3500.txt"));
        assert!(wav.ends_with("-3500.wav"));
        assert_eq!(fs::read_to_string(txt).unwrap(), "bump");
        let csv = parse(&fs::read_to_string(csv).unwrap());
        let wav = fs::read(wav).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            csv,
            [
                (3_000 * MS, Record::Mark(Mark::Hold)),
                (3_200 * MS, Record::Audio(0)),
                (3_500 * MS, Record::Mark(Mark::Start)),
                (
                    4_000 * MS,
                    Record::Reading(Reading::Rotation([0.0, 0.5, -1.25]))
                ),
                (7_000 * MS, Record::Mark(Mark::Slam)),
                (7_000 * MS, Record::Mark(Mark::Stop)),
                (7_500 * MS, Record::Reading(DOWN)),
                (8_000 * MS, Record::Reading(DOWN)),
            ]
        );
        assert_eq!(wav.len(), 48);
        assert_eq!(wav[44..], [1, 0, 0xfe, 0xff]);
    }

    #[test]
    fn cancelled_hold_stops_listening() {
        let tracer = Tracer::new(std::env::temp_dir());
        tracer.timer(Timer::Holding {
            since: 0,
            inspected: None,
        });
        assert!(tracer.audio(48_000, 0, &[1], None));
        tracer.timer(Timer::Idle);
        assert!(!tracer.audio(48_000, 1, &[1], None));
    }

    #[test]
    fn audio_timestamps_belong_to_their_audio() {
        let tracer = Tracer::new(std::env::temp_dir());
        tracer.timer(Timer::Holding {
            since: 0,
            inspected: None,
        });
        tracer.audio(48_000, 0, &[1], Some((0, 100 * MS)));
        tracer.timer(Timer::Idle);
        tracer.timer(Timer::Holding {
            since: 200 * MS,
            inspected: None,
        });
        tracer.audio(48_000, 5_000, &[1], Some((5_000, 300 * MS)));
        let csv = parse(&tracer.lock().csv(0, u64::MAX));
        let stamps: Vec<_> = csv
            .into_iter()
            .filter(|(_, r)| matches!(r, Record::Audio(_)))
            .collect();
        assert_eq!(stamps, [(300 * MS, Record::Audio(0))]);
    }

    #[test]
    fn disabled_saves_nothing() {
        let dir = crate::test_path("off");
        let tracer = Tracer::new(dir.clone());
        tracer.set_enabled(false);
        tracer.timer(Timer::Holding {
            since: 0,
            inspected: None,
        });
        assert!(
            !tracer.audio(48_000, 0, &[1], None),
            "listening while disabled"
        );
        tracer.timer(Timer::Running {
            start: 500 * MS,
            penalty: Penalty::None,
        });
        tracer.timer(Timer::Stopped { at: 2_000 * MS });
        tracer.reading(4_000 * MS, DOWN);
        tracer.answer("right");
        assert!(!dir.exists());
    }

    #[test]
    fn forgets_old_records() {
        let tracer = Tracer::new(std::env::temp_dir());
        tracer.reading(0, DOWN);
        tracer.reading(KEEP_NS + 1, DOWN);
        assert_eq!(parse(&tracer.lock().csv(0, u64::MAX)).len(), 1);
    }
}
