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

    /// The WCA average of the last `n`.
    pub fn wca_average(&self, n: usize) -> Option<Time> {
        self.wca_averages(n).next_back()
    }

    /// The best WCA average of `n` in a row.
    pub fn best_wca_average(&self, n: usize) -> Option<Time> {
        self.wca_averages(n).min()
    }

    /// The WCA average of every `n` solves in a row, oldest first. `n` is 3 or more.
    pub fn wca_averages(&self, n: usize) -> impl DoubleEndedIterator<Item = Time> {
        self.solves.windows(n).map(wca_average_of)
    }

    /// The plain mean of the solves that aren't DNF.
    pub fn mean(&self) -> Option<Time> {
        let times: Vec<Time> = self
            .solves
            .iter()
            .map(Solve::time)
            .filter(|&t| t != Time::Dnf)
            .collect();
        (!times.is_empty()).then(|| mean(&times))
    }
}

impl Time {
    /// The time in ms, `None` for DNF.
    pub fn ms(self) -> Option<u64> {
        match self {
            Time::Ms(ms) => Some(ms),
            Time::Dnf => None,
        }
    }
}

/// WCA average of `solves`: drop the best and worst 5% (rounded up), mean the rest.
fn wca_average_of(solves: &[Solve]) -> Time {
    let mut times: Vec<Time> = solves.iter().map(Solve::time).collect();
    times.sort_unstable();
    let trim = times.len().div_ceil(20);
    mean(&times[trim..times.len() - trim])
}

/// The mean of `times`, rounded to the ms. DNF if any is. `times` isn't empty.
fn mean(times: &[Time]) -> Time {
    let len = times.len() as u64;
    let sum: Option<u64> = times.iter().map(|t| t.ms()).sum();
    sum.map_or(Time::Dnf, |sum| Time::Ms((sum + len / 2) / len))
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
        assert_eq!(s.wca_average(5), Some(Time::Ms(11_000)));
        assert_eq!(s.wca_average(12), None);
        assert_eq!(s.best(), Some(Time::Ms(9_000)));

        let s = session(&[
            (10_000, Dnf),
            (12_000, ok),
            (11_000, PlusTwo),
            (9_000, ok),
            (10_000, ok),
        ]);
        assert_eq!(s.wca_average(5), Some(Time::Ms(11_667)));

        let s = session(&[
            (10_000, Dnf),
            (12_000, Dnf),
            (11_000, ok),
            (9_000, ok),
            (10_000, ok),
        ]);
        assert_eq!(s.wca_average(5), Some(Time::Dnf));
    }

    #[test]
    fn best_wca_average_and_mean() {
        let ok = Penalty::None;
        let s = session(&[
            (20_000, ok),
            (10_000, ok),
            (11_000, ok),
            (12_000, ok),
            (13_000, Dnf),
            (30_000, ok),
            (14_000, ok),
        ]);
        let averages: Vec<Time> = s.wca_averages(5).collect();
        let ms = Time::Ms;
        assert_eq!(averages, [ms(14_333), ms(17_667), ms(18_667)]);
        assert_eq!(s.best_wca_average(5), Some(ms(14_333)));
        assert_eq!(s.wca_average(5), Some(ms(18_667)));
        assert_eq!(s.best_wca_average(12), None);
        assert_eq!(s.mean(), Some(ms(16_167)));
        assert_eq!(session(&[(9_000, Dnf)]).mean(), None);
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
