//! Replays solve traces recorded on the phone (see `src/trace.rs`) through the slam detector
//! and the timer, and checks each against `traces/labels.txt`. The traces aren't tracked, so
//! this passes without checking anything when they're missing.

use cube_rs::gesture::Detector;
use cube_rs::session::Penalty;
use cube_rs::timer::Timer;
use cube_rs::trace::{Mark, Reading, Record, parse};
use std::fs;
use std::path::Path;

const MS: u64 = 1_000_000;
/// How far a slam stop may be from its labelled time.
const TOLERANCE_MS: u64 = 20;
/// A screen tap jolts the phone up to this long before the touch registers.
const TAP_JOLT_MS: u64 = 60;

/// When a slam stops the solve, and when the recorded solve stopped, in ms after the start.
fn replay(csv: &str) -> (Option<u64>, u64) {
    let mut detector = Detector::default();
    let mut timer = Timer::default();
    let (mut start, mut stop, mut slam) = (0, 0, None);
    for (t, record) in parse(csv) {
        match record {
            Record::Mark(Mark::Start) => {
                start = t;
                timer = Timer::Running {
                    start: t,
                    penalty: Penalty::None,
                };
            }
            Record::Mark(Mark::Stop) => stop = t,
            Record::Reading(Reading::Acceleration(a)) => {
                if let Some((ms, _)) = detector.acceleration(t, a).and_then(|t| timer.slam(t)) {
                    slam = Some(ms);
                }
            }
            _ => {}
        }
    }
    (slam, (stop - start) / MS)
}

/// Whether a replay meets its label: a time in ms, `tap` or `never` (see `labels.txt`).
fn meets(label: &str, slam: Option<u64>, stop: u64) -> bool {
    match label {
        "never" => slam.is_none(),
        "tap" => slam.is_none_or(|ms| ms + TAP_JOLT_MS >= stop),
        ms => {
            let ms: u64 = ms.parse().unwrap();
            slam.is_some_and(|slam| slam.abs_diff(ms) <= TOLERANCE_MS)
        }
    }
}

#[test]
fn recorded_solves() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("traces");
    let Ok(labels) = fs::read_to_string(dir.join("labels.txt")) else {
        eprintln!("no traces/labels.txt");
        return;
    };
    let mut failed = Vec::new();
    for line in labels
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let (name, label) = line.split_once(' ').unwrap();
        let (slam, stop) = replay(&fs::read_to_string(dir.join(name)).unwrap());
        let ok = meets(label, slam, stop);
        println!(
            "{} {name}: {label}, slam {slam:?}",
            if ok { "ok  " } else { "FAIL" }
        );
        if !ok {
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "{} failed: {failed:?}", failed.len());
}
