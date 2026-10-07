//! The network diagram's per-project state, kept on this computer: the hand-placed cards (the web keeps them in
//! the user's module preferences, `networkLayout`), the viewport, the time-scaled mode and snap-to-grid (the
//! web's `localStorage` keys `office:net:vp|timescale|snap:<project>`). One small JSON file per project in the
//! user's data folder (`<data>/projects/network/<id>.json`; the sandbox's under `KUBUNO_SANDBOX_DIR`).

use std::path::PathBuf;

use kubuno_office_projects_core::network::{Pos, Viewport};
use serde::{Deserialize, Serialize};

/// What is remembered for one project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkState {
    #[serde(default)]
    pub manual: Pos,
    #[serde(default)]
    pub viewport: Option<Viewport>,
    #[serde(default)]
    pub timescale: bool,
    #[serde(default)]
    pub snap: bool,
}

/// Where the states live.
pub struct NetworkStore {
    dir: PathBuf,
}

impl NetworkStore {
    pub fn for_user() -> Option<Self> {
        let dir = kubuno_desktop_account::paths::user_data_dir().ok()?.join("projects").join("network");
        Some(Self { dir })
    }

    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> PathBuf {
        let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
        self.dir.join(format!("{safe}.json"))
    }

    /// The state of a project (the default when none was kept or it is unreadable); a viewport out of
    /// range is dropped, as the web drops an invalid stored one.
    pub fn read(&self, id: &str) -> NetworkState {
        let mut s: NetworkState = std::fs::read(self.path(id)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        s.viewport = s.viewport.and_then(Viewport::valid);
        s.manual.retain(|_, p| p.x.is_finite() && p.y.is_finite());
        for p in s.manual.values_mut() {
            p.x = p.x.round();
            p.y = p.y.round();
        }
        s
    }

    /// Writes beside, then renames.
    pub fn write(&self, id: &str, s: &NetworkState) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("network : {e}"))?;
        let bytes = serde_json::to_vec(s).map_err(|e| format!("network : {e}"))?;
        let path = self.path(id);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes).map_err(|e| format!("network : {e}"))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("network : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kubuno_office_projects_core::network::Pt;

    #[test]
    fn round_trips_and_drops_a_bad_viewport() {
        let dir = std::env::temp_dir().join(format!("kubuno-projects-net-{}", std::process::id()));
        let store = NetworkStore::at(dir.clone());
        assert_eq!(store.read("p"), NetworkState::default());
        let mut s = NetworkState { timescale: true, viewport: Some(Viewport { tx: 1.0, ty: 2.0, scale: 9.0 }), ..NetworkState::default() };
        s.manual.insert("a".into(), Pt::new(10.4, 20.6));
        store.write("p", &s).expect("write");
        let back = store.read("p");
        assert!(back.timescale);
        assert_eq!(back.viewport, None);
        assert_eq!(back.manual["a"], Pt::new(10.0, 21.0));
        let _ = std::fs::remove_dir_all(dir);
    }
}
