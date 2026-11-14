//! Named sessions, saved as `<name>.jsonl` in one directory. One is open at a time.

use crate::session::Session;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct Sessions {
    dir: PathBuf,
    /// Every session's name, sorted.
    names: Vec<String>,
    name: String,
    current: Session,
}

impl Sessions {
    /// Opens session `name` in `dir`, creating either if missing.
    pub fn open(dir: PathBuf, name: &str) -> Self {
        if let Err(e) = fs::create_dir_all(&dir) {
            log::error!("creating {}: {e}", dir.display());
        }
        let mut sessions = Self {
            dir,
            names: Vec::new(),
            name: String::new(),
            current: Session::default(),
        };
        sessions.switch(name);
        sessions
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// The open session's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn current(&self) -> &Session {
        &self.current
    }

    pub fn current_mut(&mut self) -> &mut Session {
        &mut self.current
    }

    /// Opens session `name`, creating it if new.
    pub fn switch(&mut self, name: &str) {
        self.current = Session::open(&self.path(name));
        self.name = name.into();
        self.list();
    }

    /// Renames the open session.
    pub fn rename(&mut self, to: &str) {
        // The open file stays open under its new name.
        match fs::rename(self.path(&self.name), self.path(to)) {
            Ok(()) => {
                self.name = to.into();
                self.list();
            }
            Err(e) => log::error!("renaming {} to {to}: {e}", self.name),
        }
    }

    /// Deletes the open session and opens another. The last session can't be deleted.
    pub fn delete(&mut self) {
        let Some(other) = self.names.iter().find(|&n| *n != self.name).cloned() else {
            return;
        };
        match fs::remove_file(self.path(&self.name)) {
            Ok(()) => self.switch(&other),
            Err(e) => log::error!("deleting {}: {e}", self.name),
        }
    }

    /// `name` can name a new session: a plain file name that isn't taken.
    pub fn available(&self, name: &str) -> bool {
        let valid =
            (1..=100).contains(&name.len()) && !name.starts_with('.') && !name.contains('/');
        valid && !self.names.iter().any(|n| n == name)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.jsonl"))
    }

    fn list(&mut self) {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) => return log::error!("listing {}: {e}", self.dir.display()),
        };
        self.names = entries
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_stem()?.to_str()?.to_owned();
                (path.extension()? == "jsonl").then_some(name)
            })
            .collect();
        self.names.sort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Solve;

    fn times(sessions: &Sessions) -> Vec<u64> {
        sessions.current().solves().iter().map(|s| s.ms).collect()
    }

    #[test]
    fn keeps_each_sessions_solves() {
        let dir = crate::test_path("sessions");
        let mut s = Sessions::open(dir.clone(), "Default");
        s.current_mut().push(Solve::new(1000, "R".into()));
        s.switch("OH");
        s.current_mut().push(Solve::new(2000, "U".into()));
        assert_eq!(s.names(), ["Default", "OH"]);

        let mut s = Sessions::open(dir.clone(), "Default");
        assert_eq!(times(&s), [1000]);
        s.switch("OH");
        assert_eq!(times(&s), [2000]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renames_and_deletes() {
        let dir = crate::test_path("rename");
        let mut s = Sessions::open(dir.clone(), "Default");
        s.current_mut().push(Solve::new(1000, "R".into()));
        s.rename("3x3");
        // Saved under the new name.
        s.current_mut().push(Solve::new(2000, "U".into()));
        assert_eq!(s.names(), ["3x3"]);
        assert_eq!(times(&Sessions::open(dir.clone(), "3x3")), [1000, 2000]);

        s.delete();
        assert_eq!(s.names(), ["3x3"], "the last session stays");
        s.switch("OH");
        s.delete();
        assert_eq!(s.names(), ["3x3"]);
        assert_eq!(times(&s), [1000, 2000]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn available_names() {
        let dir = crate::test_path("available");
        let s = Sessions::open(dir.clone(), "Default");
        assert!(s.available("OH 3x3"));
        for taken_or_bad in ["Default", "", ".hidden", "a/b", &"x".repeat(101)] {
            assert!(!s.available(taken_or_bad), "{taken_or_bad:?}");
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
