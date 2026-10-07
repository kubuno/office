//! The journal of an open server project: its last server copy and the changes not yet sent
//! (`kubuno_office_projects_core::editor::Journal`), one file per project in the user's data folder
//! (`<data>/projects/journal/<id>.json`; the sandbox's under `KUBUNO_SANDBOX_DIR`). Written after every
//! command, so a crash or a lost connection loses nothing; cleared once everything is on the server.

use std::path::PathBuf;

use kubuno_office_projects_core::editor::Journal;

pub struct FileJournal {
    dir: PathBuf,
}

impl FileJournal {
    pub fn for_user() -> Option<Self> {
        let dir = kubuno_desktop_account::paths::user_data_dir().ok()?.join("projects").join("journal");
        Some(Self { dir })
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.json"))
    }

    /// Writes beside, then renames: a crash mid-write never leaves a torn journal.
    pub fn write(&self, id: &str, j: &Journal) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("journal : {e}"))?;
        let bytes = serde_json::to_vec(j).map_err(|e| format!("journal : {e}"))?;
        let path = self.path(id);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| format!("journal : {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("journal : {e}"))
    }

    pub fn read(&self, id: &str) -> Option<Journal> {
        let bytes = std::fs::read(self.path(id)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn clear(&self, id: &str) {
        let _ = std::fs::remove_file(self.path(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!("kubuno-projects-journal-{}", std::process::id()));
        let j = FileJournal::at(dir.clone());
        let mut journal = Journal::default();
        journal.base.project.id = "p/1".into();
        j.write("p/1", &journal).expect("written");
        assert_eq!(j.read("p/1").map(|x| x.base.project.id), Some("p/1".into()));
        j.clear("p/1");
        assert!(j.read("p/1").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
