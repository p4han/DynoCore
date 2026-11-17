//! Looks at the PC so the dashboard can show what each change would do ("Now: ...").
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Facts {
    pub plan: String,
    pub hibernation: bool,
    pub transparency: bool,
    pub sysmain: Option<bool>,
    pub wsearch: Option<bool>,
    pub paging_exec_disabled: bool,
    pub large_system_cache: bool,
    pub user_temp: u64,
    pub win_temp: u64,
    pub update_cache: u64,
    pub delivery_cache: u64,
    pub browser_cache: u64,
    pub crash_dumps: u64,
}

fn run_out(exe: &str, args: &[&str]) -> Option<String> {
    Command::new(exe)
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

pub fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            if let Ok(m) = e.metadata() {
                if m.is_dir() {
                    total += dir_size(&e.path());
                } else {
                    total += m.len();
                }
            }
        }
    }
    total
}

pub fn windows_dir() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
}

pub fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}

pub fn temp_total() -> u64 {
    let win = windows_dir();
    dir_size(&std::env::temp_dir())
        + dir_size(&Path::new(&win).join("Temp"))
        + dir_size(&Path::new(&win).join(r"SoftwareDistribution\Download"))
        + dir_size(&Path::new(&win).join(r"SoftwareDistribution\DeliveryOptimization\Cache"))
}

fn browser_cache_size() -> u64 {
    let mut total = 0u64;
    if let Some(local) = local_app_data() {
        total += dir_size(&local.join(r"Google\Chrome\User Data\Default\Cache"));
        total += dir_size(&local.join(r"Google\Chrome\User Data\Default\Code Cache"));
        total += dir_size(&local.join(r"Microsoft\Edge\User Data\Default\Cache"));
        total += dir_size(&local.join(r"Microsoft\Edge\User Data\Default\Code Cache"));
        total += dir_size(&local.join(r"Mozilla\Firefox\Profiles"));
    }
    total
}

fn service_running(name: &str) -> Option<bool> {
    let out = run_out("sc", &["query", name])?;
    if out.contains("1060") {
        return None;
    }
    Some(out.contains("RUNNING"))
}

fn check_reg_dword(key: &str, name: &str) -> Option<u32> {
    let out = run_out("reg", &["query", key, "/v", name])?;
    for line in out.lines() {
        if line.contains(name) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let v = parts[2].trim_start_matches("0x");
                return u32::from_str_radix(v, 16).ok();
            }
        }
    }
    None
}

pub fn facts() -> Facts {
    let plan = run_out("powercfg", &["/getactivescheme"])
        .and_then(|s| {
            let a = s.rfind('(')?;
            let b = s.rfind(')')?;
            if b > a {
                Some(s[a + 1..b].trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Balanced".to_string());

    let sys_drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
    let hiber_path = format!(r"{}\hiberfil.sys", sys_drive);
    let hibernation = Path::new(&hiber_path).exists()
        || run_out("powercfg", &["/a"]).map_or(false, |s| s.contains("Hibernate") && !s.contains("Hibernation has not been enabled"));

    let transparency = check_reg_dword(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "EnableTransparency",
    ).map_or(true, |v| v != 0);

    let mm = r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management";
    let paging_exec_disabled = check_reg_dword(mm, "DisablePagingExecutive").map_or(false, |v| v == 1);
    let large_system_cache = check_reg_dword(mm, "LargeSystemCache").map_or(false, |v| v == 1);

    let win = windows_dir();
    let win_p = Path::new(&win);
    let crash_dumps = dir_size(&win_p.join("Minidump"))
        + dir_size(&win_p.join("LiveKernelReports"))
        + if win_p.join("MEMORY.DMP").exists() { win_p.join("MEMORY.DMP").metadata().map(|m| m.len()).unwrap_or(0) } else { 0 };

    Facts {
        plan,
        hibernation,
        transparency,
        sysmain: service_running("SysMain"),
        wsearch: service_running("WSearch"),
        paging_exec_disabled,
        large_system_cache,
        user_temp: dir_size(&std::env::temp_dir()),
        win_temp: dir_size(&win_p.join("Temp")),
        update_cache: dir_size(&win_p.join(r"SoftwareDistribution\Download")),
        delivery_cache: dir_size(&win_p.join(r"SoftwareDistribution\DeliveryOptimization\Cache")),
        browser_cache: browser_cache_size(),
        crash_dumps,
    }
}
