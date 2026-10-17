//! Persistent display preferences beside the executable.

use serde::{Deserialize, Serialize};
use std::path::Path;

// Unknown fields (such as the import interval of earlier versions) are ignored.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub animations: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { animations: true }
    }
}

impl Preferences {
    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("preferences.json")).ok()
            .and_then(|data| serde_json::from_slice::<Self>(&data).ok())
            .unwrap_or_default()
    }

    pub fn save(self, dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(&self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("preferences.json"), data).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_survive_restart() {
        let dir = crate::logfile::tests::temp("preferences");
        assert_eq!(Preferences::load(&dir), Preferences::default());
        let prefs = Preferences { animations: false };
        prefs.save(&dir).unwrap();
        assert_eq!(Preferences::load(&dir), prefs);
        std::fs::write(dir.join("preferences.json"), br#"{"animations":false,"refresh_seconds":5}"#).unwrap();
        assert_eq!(Preferences::load(&dir), prefs, "files from earlier versions still load");
        std::fs::write(dir.join("preferences.json"), b"{broken").unwrap();
        assert_eq!(Preferences::load(&dir), Preferences::default());
        assert_eq!(serde_json::from_str::<Preferences>("{}").unwrap(), Preferences::default());
    }
}
