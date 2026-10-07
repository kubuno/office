//! The crash journal: the open presentation's local copy AND the server copy it was last saved against, written
//! to the user's data folder (`<data>/presentations/journal/<id>.json`, the sandbox's under
//! `KUBUNO_SANDBOX_DIR`) before any server write and cleared once the server holds everything. With both
//! copies, a restart (or a reconnection) knows exactly what is unsaved and still guards every slide.

use std::collections::HashMap;
use std::path::PathBuf;

use kubuno_office_slides_core::model::Deck;
use kubuno_office_slides_core::obj::{Obj, Slot};

/// One journal entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub local: Deck,
    pub base: Deck,
}

impl Entry {
    pub fn to_json(&self) -> String {
        let mut o = Obj::new();
        o.set_slot("local", Slot::raw(&self.local.to_bundle()));
        o.set_slot("base", Slot::raw(&self.base.to_bundle()));
        o.to_json()
    }

    pub fn parse(text: &str) -> Option<Entry> {
        let o = Obj::parse(text).ok()?;
        let local = Deck::from_json(o.slot("local")?.text().as_bytes()).ok()?;
        let base = Deck::from_json(o.slot("base")?.text().as_bytes()).ok()?;
        Some(Entry { local, base })
    }
}

pub trait Journal {
    fn write(&mut self, id: &str, entry: &Entry) -> Result<(), String>;
    fn read(&mut self, id: &str) -> Option<Entry>;
    fn clear(&mut self, id: &str);
}

/// The journal on disk.
pub struct FileJournal {
    dir: PathBuf,
}

impl FileJournal {
    pub fn for_user() -> Option<FileJournal> {
        let dir = kubuno_desktop_account::paths::user_data_dir().ok()?.join("presentations").join("journal");
        Some(FileJournal { dir })
    }

    pub fn at(dir: PathBuf) -> FileJournal {
        FileJournal { dir }
    }

    /// A file name valid on every OS whatever the id holds.
    fn path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.json"))
    }
}

impl Journal for FileJournal {
    fn write(&mut self, id: &str, entry: &Entry) -> Result<(), String> {
        let path = self.path(id);
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("journal : {e}"))?;
        // Written beside, then renamed: a crash mid-write never leaves a torn journal.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, entry.to_json()).map_err(|e| format!("journal : {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("journal : {e}"))
    }

    fn read(&mut self, id: &str) -> Option<Entry> {
        Entry::parse(&std::fs::read_to_string(self.path(id)).ok()?)
    }

    fn clear(&mut self, id: &str) {
        let _ = std::fs::remove_file(self.path(id));
    }
}

/// A journal in memory (tests).
#[derive(Default)]
pub struct MemJournal(pub HashMap<String, String>);

impl Journal for MemJournal {
    fn write(&mut self, id: &str, entry: &Entry) -> Result<(), String> {
        self.0.insert(id.to_string(), entry.to_json());
        Ok(())
    }
    fn read(&mut self, id: &str) -> Option<Entry> {
        self.0.get(id).and_then(|t| Entry::parse(t))
    }
    fn clear(&mut self, id: &str) {
        self.0.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_journal_round_trips_on_disk_and_clears() {
        let d = crate::api::session::tests::deck(&["a", "b"]);
        let mut local = d.clone();
        local.slides[1].hidden = true;
        local.slides[0].data.set_notes("n");
        let dir = std::env::temp_dir().join(format!("kubuno-pres-journal-{}", std::process::id()));
        let mut j = FileJournal::at(dir.clone());
        assert!(j.read("x/y").is_none());
        j.write("x/y", &Entry { local: local.clone(), base: d.clone() }).expect("written");
        let e = j.read("x/y").expect("read");
        assert_eq!(e.local.slides, local.slides);
        assert_eq!(e.base.slides, d.slides);
        assert!(j.path("x/y").ends_with("x_y.json"));
        j.clear("x/y");
        assert!(j.read("x/y").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
