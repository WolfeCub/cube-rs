//! Solves saved as one JSON line each. Only the last solve ever changes, so a change appends a
//! line or cuts the last one off: the file is never rewritten.

use crate::session::Solve;
use nanoserde::{DeJson, SerJson};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::Path;

#[derive(Debug)]
pub struct Store {
    file: File,
    /// Where each solve's line ends.
    ends: Vec<u64>,
}

impl Store {
    /// Opens `path`, creating it if missing, and reads its solves. Anything from the first line
    /// that can't be read (a write cut short) is cut off, after copying the file to `<path>.bad`.
    pub fn open(path: &Path) -> io::Result<(Self, Vec<Solve>)> {
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;

        let mut store = Self {
            file,
            ends: Vec::new(),
        };
        let mut solves = Vec::new();
        for line in bytes.split_inclusive(|&b| b == b'\n') {
            let Some(solve) = parse(line) else { break };
            store.ends.push(store.len() + line.len() as u64);
            solves.push(solve);
        }
        if store.len() < bytes.len() as u64 {
            log::error!("{}: unreadable from byte {}", path.display(), store.len());
            fs::copy(path, path.with_extension("bad"))?;
            store.file.set_len(store.len())?;
        }
        Ok((store, solves))
    }

    pub fn push(&mut self, solve: &Solve) {
        let line = solve.serialize_json() + "\n";
        self.ends.push(self.len() + line.len() as u64);
        if let Err(e) = self.file.write_all(line.as_bytes()) {
            log::error!("saving a solve: {e}");
        }
    }

    pub fn pop(&mut self) {
        if self.ends.pop().is_some()
            && let Err(e) = self.file.set_len(self.len())
        {
            log::error!("removing a solve: {e}");
        }
    }

    fn len(&self) -> u64 {
        self.ends.last().copied().unwrap_or(0)
    }
}

/// A whole line: one solve and its newline.
fn parse(line: &[u8]) -> Option<Solve> {
    let line = std::str::from_utf8(line.strip_suffix(b"\n")?).ok()?;
    Solve::deserialize_json(line).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Penalty;

    fn ms(solves: &[Solve]) -> Vec<u64> {
        solves.iter().map(|s| s.ms).collect()
    }

    #[test]
    fn keeps_solves() {
        let path = crate::test_path("keeps");
        let (mut store, solves) = Store::open(&path).unwrap();
        assert!(solves.is_empty());
        store.push(&Solve::new(1000, "R U".into()));
        store.push(&Solve::new(2000, "F2".into()));
        store.pop();
        let dnf = Solve {
            penalty: Penalty::Dnf,
            ..Solve::new(3000, "D'".into())
        };
        store.push(&dnf);

        let (mut store, solves) = Store::open(&path).unwrap();
        assert_eq!(ms(&solves), [1000, 3000]);
        assert_eq!(solves[1], dnf);
        store.pop();
        store.pop();
        assert!(fs::read(&path).unwrap().is_empty());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cuts_off_a_partial_line() {
        let path = crate::test_path("partial");
        let (mut store, _) = Store::open(&path).unwrap();
        store.push(&Solve::new(1000, "R U".into()));
        let good = fs::read(&path).unwrap();
        let bad = [&good[..], br#"{"ms":20"#].concat();
        fs::write(&path, &bad).unwrap();

        let (_, solves) = Store::open(&path).unwrap();
        assert_eq!(ms(&solves), [1000]);
        assert_eq!(fs::read(&path).unwrap(), good);
        assert_eq!(fs::read(path.with_extension("bad")).unwrap(), bad);
        fs::remove_file(path.with_extension("bad")).unwrap();
        fs::remove_file(path).unwrap();
    }
}
