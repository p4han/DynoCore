//! Tiny settings file: %APPDATA%\Dyno\settings.ini (plain key=value lines).
use crate::tweaks;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub struct Settings {
    /// Working sets, system cache, modified pages, standby list.
    pub areas: [bool; 4],
    pub auto: bool,
    /// Auto-clean when RAM in use reaches this percent.
    pub thr: u32,
    /// Which changes are ticked on the dashboard.
    pub tweaks: [bool; tweaks::COUNT],
}

impl Settings {
    pub fn new() -> Self {
        let mut t = [false; tweaks::COUNT];
        for i in 0..tweaks::COUNT {
            t[i] = tweaks::TWEAKS[i].risk == tweaks::SAFE;
        }
        Settings { areas: [true; 4], auto: true, thr: 65, tweaks: t }
    }
}

fn dir() -> Option<PathBuf> {
    let mut p = PathBuf::from(std::env::var_os("APPDATA")?);
    p.push("Dyno");
    Some(p)
}

pub fn load() -> Settings {
    let mut s = Settings::new();
    if let Some(d) = dir() {
        if let Ok(text) = fs::read_to_string(d.join("settings.ini")) {
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    let v = v.trim();
                    match k.trim() {
                        "areas" => {
                            for (i, c) in v.chars().take(4).enumerate() {
                                s.areas[i] = c == '1';
                            }
                        }
                        "auto" => s.auto = v == "1",
                        "thr" => {
                            if let Ok(n) = v.parse::<u32>() {
                                s.thr = n.clamp(40, 95);
                            }
                        }
                        "sel" => {
                            for (i, c) in v.chars().take(tweaks::COUNT).enumerate() {
                                s.tweaks[i] = c == '1';
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    s
}

pub fn save(s: &Settings) {
    if let Some(d) = dir() {
        let _ = fs::create_dir_all(&d);
        let areas: String = s.areas.iter().map(|a| if *a { '1' } else { '0' }).collect();
        let sel: String = s.tweaks.iter().map(|t| if *t { '1' } else { '0' }).collect();
        let text = format!("areas={}\nauto={}\nthr={}\nsel={}\n", areas, if s.auto { 1 } else { 0 }, s.thr, sel);
        let _ = fs::write(d.join("settings.ini"), text);
    }
}
