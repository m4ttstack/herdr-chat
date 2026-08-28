#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, PartialEq, Clone, Copy, Debug)]
pub enum SigninPref {
    Ask,
    Always,
    Never,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Broadcast {
    pub at: i64,
    pub message: String,
    pub recipients: Vec<Recipient>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Recipient {
    pub pane_id: String,
    pub handle: Option<String>,
    pub delivered: String,
}

#[allow(dead_code)]
pub fn state_dir() -> PathBuf {
    PathBuf::from(std::env::var("HERDR_PLUGIN_STATE_DIR").unwrap_or_else(|_| ".".to_string()))
}

pub fn get_pref(dir: &Path, repo: &str) -> SigninPref {
    let pref_file = dir.join("signin_prefs.json");
    match fs::read_to_string(&pref_file) {
        Ok(content) => {
            if let Ok(prefs) =
                serde_json::from_str::<std::collections::HashMap<String, SigninPref>>(&content)
            {
                prefs.get(repo).copied().unwrap_or(SigninPref::Ask)
            } else {
                SigninPref::Ask
            }
        }
        Err(_) => SigninPref::Ask,
    }
}

pub fn set_pref(dir: &Path, repo: &str, pref: SigninPref) -> std::io::Result<()> {
    let pref_file = dir.join("signin_prefs.json");
    let mut prefs = if let Ok(content) = fs::read_to_string(&pref_file) {
        serde_json::from_str::<std::collections::HashMap<String, SigninPref>>(&content)
            .unwrap_or_default()
    } else {
        std::collections::HashMap::new()
    };
    prefs.insert(repo.to_string(), pref);
    let json = serde_json::to_string(&prefs)?;
    fs::write(&pref_file, json)
}

pub fn push_pending(dir: &Path, pane_id: &str) -> std::io::Result<()> {
    let pending_file = dir.join("pending.json");
    let mut pending = if let Ok(content) = fs::read_to_string(&pending_file) {
        serde_json::from_str::<Vec<String>>(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    pending.push(pane_id.to_string());
    let json = serde_json::to_string(&pending)?;
    fs::write(&pending_file, json)
}

pub fn drain_pending(dir: &Path) -> std::io::Result<Vec<String>> {
    let pending_file = dir.join("pending.json");
    let pending = if let Ok(content) = fs::read_to_string(&pending_file) {
        serde_json::from_str::<Vec<String>>(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    if pending_file.exists() {
        fs::remove_file(&pending_file)?;
    }
    Ok(pending)
}

pub fn push_broadcast(dir: &Path, b: &Broadcast) -> std::io::Result<()> {
    let broadcasts_file = dir.join("broadcasts.json");
    let mut broadcasts = if let Ok(content) = fs::read_to_string(&broadcasts_file) {
        serde_json::from_str::<Vec<Broadcast>>(&content).unwrap_or_default()
    } else {
        Vec::new()
    };
    broadcasts.push(b.clone());
    if broadcasts.len() > 50 {
        broadcasts.remove(0);
    }
    let json = serde_json::to_string(&broadcasts)?;
    fs::write(&broadcasts_file, json)
}

pub fn recent_broadcasts(dir: &Path) -> Vec<Broadcast> {
    let broadcasts_file = dir.join("broadcasts.json");
    if let Ok(content) = fs::read_to_string(&broadcasts_file) {
        if let Ok(mut broadcasts) = serde_json::from_str::<Vec<Broadcast>>(&content) {
            broadcasts.reverse();
            return broadcasts;
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pref_round_trips_per_repo_and_defaults_to_ask() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(get_pref(d.path(), "chat"), SigninPref::Ask);
        set_pref(d.path(), "chat", SigninPref::Always).unwrap();
        assert_eq!(get_pref(d.path(), "chat"), SigninPref::Always);
        assert_eq!(get_pref(d.path(), "other"), SigninPref::Ask);
    }

    #[test]
    fn pending_drains_and_clears() {
        let d = tempfile::tempdir().unwrap();
        push_pending(d.path(), "w1:p1").unwrap();
        push_pending(d.path(), "w1:p2").unwrap();
        assert_eq!(drain_pending(d.path()).unwrap(), vec!["w1:p1", "w1:p2"]);
        assert!(drain_pending(d.path()).unwrap().is_empty());
    }

    #[test]
    fn broadcasts_cap_at_fifty_newest_first() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..60 {
            push_broadcast(
                d.path(),
                &Broadcast {
                    at: i,
                    message: i.to_string(),
                    recipients: vec![],
                },
            )
            .unwrap();
        }
        let r = recent_broadcasts(d.path());
        assert_eq!(r.len(), 50);
        assert_eq!(r[0].message, "59");
    }
}
