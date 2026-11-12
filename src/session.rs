//! Solves, penalties and WCA statistics.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone)]
pub struct Solve {
    pub ms: u64,
    pub penalty: Penalty,
    pub scramble: String,
}

impl Solve {
    pub fn time(&self) -> Time {
        match self.penalty {
            Penalty::None => Time::Ms(self.ms),
            Penalty::PlusTwo => Time::Ms(self.ms + 2000),
            Penalty::Dnf => Time::Dnf,
        }
    }
}

#[derive(Debug, Default)]
pub struct Session {
    pub solves: Vec<Solve>,
}

impl Session {
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
        let solves = solves.iter().map(|&(ms, penalty)| Solve {
            ms,
            penalty,
            scramble: String::new(),
        });
        Session {
            solves: solves.collect(),
        }
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
    fn formatting() {
        assert_eq!(Time::Ms(9_876).to_string(), "9.87");
        assert_eq!(Time::Ms(62_349).to_string(), "1:02.34");
        assert_eq!(Time::Dnf.to_string(), "DNF");
    }
}
