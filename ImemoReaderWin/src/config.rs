//! Configuration and coordinate persistence for iMemo Reader
//! Saves and restores selected capture rectangle coordinates

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CaptureRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Default for CaptureRect {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            width: 800,
            height: 200,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub last_rect: Option<CaptureRect>,
    pub capture_interval_ms: Option<u64>,
}

impl AppConfig {
    fn config_path() -> PathBuf {
        if let Some(app_data) = std::env::var_os("APPDATA") {
            let mut p = PathBuf::from(app_data);
            p.push("iMemo");
            let _ = fs::create_dir_all(&p);
            p.push("config.json");
            p
        } else {
            PathBuf::from("imemo_config.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Self>(&data) {
                return cfg;
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}

