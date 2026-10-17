//! Persistent display and automatic import preferences beside the executable.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Preferences {
    pub animations: bool,
    pub refresh_seconds: u64,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { animations: true, refresh_seconds: 1 }
    }
}

impl Preferences {
    pub fn validate(self) -> Result<Self, String> {
        if !(1..=60).contains(&self.refresh_seconds) {
            return Err("自動更新の間隔は1～60秒で指定してください".into());
        }
        Ok(self)
    }

    pub fn load(dir: &Path) -> Self {
        std::fs::read(dir.join("preferences.json")).ok()
            .and_then(|data| serde_json::from_slice::<Self>(&data).ok())
            .and_then(|prefs| prefs.validate().ok())
            .unwrap_or_default()
    }

    pub fn save(self, dir: &Path) -> Result<(), String> {
        self.validate()?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let data = serde_json::to_vec_pretty(&self).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("preferences.json"), data).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_survive_restart_and_reject_invalid_intervals() {
        let dir = crate::logfile::tests::temp("preferences");
        assert_eq!(Preferences::load(&dir), Preferences::default());
        let prefs = Preferences { animations: false, refresh_seconds: 5 };
        prefs.save(&dir).unwrap();
        assert_eq!(Preferences::load(&dir), prefs);
        for seconds in [0, 61, u64::MAX] {
            assert!(Preferences { refresh_seconds: seconds, ..prefs }.save(&dir).is_err());
            assert_eq!(Preferences::load(&dir), prefs, "invalid updates leave saved preferences intact");
        }
        for seconds in [1, 2, 5, 10, 30, 60] {
            assert!(Preferences { refresh_seconds: seconds, ..prefs }.validate().is_ok());
        }
        std::fs::write(dir.join("preferences.json"), b"{broken").unwrap();
        assert_eq!(Preferences::load(&dir), Preferences::default());
        assert_eq!(serde_json::from_str::<Preferences>("{}").unwrap(), Preferences::default());
    }
}
