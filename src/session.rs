//! Solves, penalties and WCA statistics.

use crate::store::Store;
use nanoserde::{DeJson, SerJson};
use std::fmt;
use std::path::Path;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, DeJson, SerJson)]
pub enum Penalty {
    #[default]
    None,
    PlusTwo,
    Dnf,
}

/// A result after penalties. DNF sorts last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Time {
    Ms(u64),
    Dnf,
}

#[derive(Debug, Clone, PartialEq, Eq, DeJson, SerJson)]
pub struct Solve {
    pub ms: u64,
    pub penalty: Penalty,
    pub scramble: String,
    /// When it was done, in Unix seconds.
    pub at: u64,
}

impl Solve {
    /// A solve done now.
    pub fn new(ms: u64, scramble: String) -> Self {
        let at = SystemTime::UNIX_EPOCH
            .elapsed()
            .unwrap_or_default()
            .as_secs();
        Self {
            ms,
            penalty: Penalty::None,
            scramble,
            at,
        }
    }

    pub fn time(&self) -> Time {
        match self.penalty {
            Penalty::None => Time::Ms(self.ms),
            Penalty::PlusTwo => Time::Ms(self.ms + 2000),
            Penalty::Dnf => Time::Dnf,
        }
    }
}

/// Solves in order, saved to a `Store` if it has one. Only the last solve can change.
#[derive(Debug, Default)]
pub struct Session {
    solves: Vec<Solve>,
    store: Option<Store>,
}

impl Session {
    /// The solves saved in `path`. Changes are saved there too.
    pub fn open(path: &Path) -> Self {
        match Store::open(path) {
            Ok((store, solves)) => Self {
                solves,
                store: Some(store),
            },
            Err(e) => {
                log::error!("opening {}: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn solves(&self) -> &[Solve] {
        &self.solves
    }

    pub fn push(&mut self, solve: Solve) {
        if let Some(store) = &mut self.store {
            store.push(&solve);
        }
        self.solves.push(solve);
    }

    pub fn pop(&mut self) -> Option<Solve> {
        let solve = self.solves.pop()?;
        if let Some(store) = &mut self.store {
            store.pop();
        }
        Some(solve)
    }

    /// Sets the last solve's penalty.
    pub fn set_penalty(&mut self, penalty: Penalty) {
        if let Some(solve) = self.pop() {
            self.push(Solve { penalty, ..solve });
        }
    }

    pub fn best(&self) -> Option<Time> {
        self.solves.iter().map(Solve::time).min()
    }

    /// WCA average of the last `n`: drop the best and worst 5% (rounded up), mean the rest.
    pub fn average(&self, n: usize) -> Option<Time> {
        let last = self.solves.get(self.solves.len().checked_sub(n)?..)?;
        let mut times: Vec<Time> = last.iter().map(Solve::time).collect();
        times.sort();
        let trim = n.div_ceil(20);
        let counted = &times[trim..n - trim];
        let sum = counted.iter().try_fold(0, |sum, t| match t {
            Time::Ms(ms) => Some(sum + ms),
            Time::Dnf => None,
        });
        let len = counted.len() as u64;
        Some(sum.map_or(Time::Dnf, |sum| Time::Ms((sum + len / 2) / len)))
    }
}

/// `9.87`, `1:02.34`. Truncated to hundredths like WCA singles.
impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let Time::Ms(ms) = *self else {
            return f.write_str("DNF");
        };
        let cs = ms / 10;
        match cs / 6000 {
            0 => write!(f, "{}.{:02}", cs / 100, cs % 100),
            min => write!(f, "{min}:{:02}.{:02}", cs / 100 % 60, cs % 100),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Penalty::{Dnf, PlusTwo};

    fn session(solves: &[(u64, Penalty)]) -> Session {
        let mut session = Session::default();
        for &(ms, penalty) in solves {
            session.push(Solve {
                penalty,
                ..Solve::new(ms, String::new())
            });
        }
        session
    }

    #[test]
    fn averages() {
        let ok = Penalty::None;
        let s = session(&[
            (10_000, ok),
            (12_000, ok),
            (11_000, ok),
            (9_000, ok),
            (20_000, ok),
        ]);
        assert_eq!(s.average(5), Some(Time::Ms(11_000)));
        assert_eq!(s.average(12), None);
        assert_eq!(s.best(), Some(Time::Ms(9_000)));

        let s = session(&[
            (10_000, Dnf),
            (12_000, ok),
            (11_000, PlusTwo),
            (9_000, ok),
            (10_000, ok),
        ]);
        assert_eq!(s.average(5), Some(Time::Ms(11_667)));

        let s = session(&[
            (10_000, Dnf),
            (12_000, Dnf),
            (11_000, ok),
            (9_000, ok),
            (10_000, ok),
        ]);
        assert_eq!(s.average(5), Some(Time::Dnf));
    }

    #[test]
    fn saves_changes() {
        let path = crate::test_path("session");
        let mut s = Session::open(&path);
        s.push(Solve::new(10_000, "R U".into()));
        s.push(Solve::new(12_000, "F2".into()));
        s.set_penalty(PlusTwo);
        s.push(Solve::new(9_000, "D'".into()));
        s.pop();
        assert_eq!(s.solves[1].time(), Time::Ms(14_000));
        assert_eq!(Session::open(&path).solves, s.solves);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn formatting() {
        assert_eq!(Time::Ms(9_876).to_string(), "9.87");
        assert_eq!(Time::Ms(62_349).to_string(), "1:02.34");
        assert_eq!(Time::Dnf.to_string(), "DNF");
    }
}
