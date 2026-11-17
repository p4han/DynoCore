//! The changes Dyno can make: what they're called, what they do, and how they're applied.
use crate::{mem, scan};
use std::os::windows::process::CommandExt;
use std::process::Command;

pub const SAFE: u8 = 0;
pub const MID: u8 = 1;
pub const HIGH: u8 = 2;

pub const CAT_NAMES: [&str; 6] = ["Optimize", "CPU", "Memory", "Cleanup", "Gaming", "Privacy"];
pub const CAT_START: [usize; 6] = [0, 7, 15, 23, 30, 36];
pub const CAT_END: [usize; 6] = [7, 15, 23, 30, 36, 43];
pub const COUNT: usize = 43;

#[allow(dead_code)]
pub struct TweakDef {
    pub name: &'static str,
    /// Shown under the name when Dyno can't read the real state.
    pub now: &'static str,
    /// Shown on the results screen after it worked.
    pub done: &'static str,
    pub risk: u8,
    pub default_on: bool,
    pub reboot: bool,
}

const fn t(
    name: &'static str,
    now: &'static str,
    done: &'static str,
    risk: u8,
    default_on: bool,
    reboot: bool,
) -> TweakDef {
    TweakDef {
        name,
        now,
        done,
        risk,
        default_on,
        reboot,
    }
}

pub static TWEAKS: [TweakDef; COUNT] = [
    // --- Optimize (0..7) ---
    t(
        "Disable Background Apps",
        "Stops Store/UWP apps running silently",
        "Background apps disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Hibernation",
        "Frees SSD space (hiberfil.sys)",
        "Hibernation disabled and file removed",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Window Transparency",
        "Removes Acrylic/Mica blur to save GPU",
        "Transparency effects disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Remove Startup Delay",
        "Eliminates 10s login delay for desktop apps",
        "Startup delay eliminated",
        SAFE,
        true,
        true,
    ),
    t(
        "Instant Context Menus",
        "Removes 400ms delay on popup menus",
        "Context menu show delay set to 0",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Crash Dumps & WER",
        "Prevents Windows creating large crash memory dumps",
        "Windows Error Reporting and dumps disabled",
        MID,
        false,
        false,
    ),
    t(
        "Disable Windows Search Indexer",
        "Stops heavy background RAM & disk indexing",
        "Windows Search indexing service stopped",
        HIGH,
        false,
        false,
    ),

    // --- CPU (7..15) ---
    t(
        "Disable Core Parking",
        "Keeps all CPU cores awake under load",
        "Core parking disabled (CPMINCORES 100)",
        SAFE,
        true,
        false,
    ),
    t(
        "Aggressive CPU Boost",
        "Ramps clock speed to max immediately",
        "Aggressive CPU boost mode enabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Performance-Leaning EPP",
        "Energy preference favors speed (10%)",
        "Energy Performance Preference set to 10",
        SAFE,
        true,
        false,
    ),
    t(
        "High Game Task Priority",
        "Higher CPU & I/O priority for games",
        "MMCSS game task priority set to High",
        SAFE,
        true,
        false,
    ),
    t(
        "100% Minimum CPU State",
        "Prevents downclocking during game lulls",
        "Minimum processor throttle set to 100%",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable PCIe Link Power Saving",
        "Stops GPU/NVMe lanes entering sleep",
        "PCIe ASPM link state power saving off",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Dynamic Tick (High Precision)",
        "Stops timer frequency fluctuations in games",
        "Dynamic tick disabled (bcdedit)",
        MID,
        false,
        true,
    ),
    t(
        "Disable Spectre & Meltdown Mitigations",
        "Reclaims 5-15% raw CPU speed (Less hardware isolation)",
        "CPU speculative execution mitigations disabled",
        HIGH,
        false,
        true,
    ),

    // --- Memory (15..23) ---
    t(
        "Enable Memory Compression",
        "Compresses inactive RAM pages",
        "Memory compression enabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Combine Duplicate Pages",
        "Merges identical memory pages to free RAM",
        "Page combining activated",
        SAFE,
        true,
        false,
    ),
    t(
        "Flush Modified Pages",
        "Writes modified pages to disk for reuse",
        "Modified page list flushed to disk",
        SAFE,
        true,
        false,
    ),
    t(
        "Trim Working Sets on Idle Apps",
        "Forces immediate purge of idle app memory",
        "Idle process working sets trimmed",
        SAFE,
        true,
        false,
    ),
    t(
        "Lock Kernel in Physical RAM",
        "Stops Windows paging kernel drivers to disk",
        "DisablePagingExecutive enabled",
        MID,
        false,
        true,
    ),
    t(
        "Turn off SysMain (Superfetch)",
        "Stops background RAM preloading on SSDs",
        "SysMain service stopped and disabled",
        MID,
        false,
        false,
    ),
    t(
        "Disable Memory Compression (Low Latency)",
        "Removes CPU decompression cycles on 32GB+ RAM",
        "Memory compression disabled",
        MID,
        false,
        false,
    ),
    t(
        "Enable Large System Cache",
        "Allocates RAM to file cache (Caution: can starve games)",
        "LargeSystemCache enabled",
        HIGH,
        false,
        true,
    ),

    // --- Cleanup (23..30) ---
    t(
        "System & User Temp Files",
        "Clears leftover installer and temp files",
        "System and user temp files cleared",
        SAFE,
        true,
        false,
    ),
    t(
        "Windows Update Cache",
        "Purges old downloaded update installers",
        "SoftwareDistribution download cache cleared",
        SAFE,
        true,
        false,
    ),
    t(
        "Delivery Optimization Cache",
        "Deletes cached peer update fragments",
        "Delivery optimization cache deleted",
        SAFE,
        true,
        false,
    ),
    t(
        "Browser Caches",
        "Clears Chrome, Edge, Firefox web cache",
        "Browser cache directories cleared",
        SAFE,
        true,
        false,
    ),
    t(
        "Empty Recycle Bin",
        "Permanently removes deleted files on all drives",
        "Recycle Bin emptied",
        SAFE,
        true,
        false,
    ),
    t(
        "Component Store Cleanup",
        "Purges superseded update backups (Slow)",
        "Component store (WinSxS) cleaned",
        MID,
        false,
        false,
    ),
    t(
        "Purge Crash Dumps & LiveKernelReports",
        "Purges Minidump, LiveKernelReports and MEMORY.DMP",
        "Kernel crash dump files deleted",
        HIGH,
        false,
        false,
    ),

    // --- Gaming (30..36) ---
    t(
        "Disable Sticky Keys Shortcut",
        "Stops 5x Shift prompt interrupting games",
        "Sticky keys shortcut disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Windowed Game Optimizations",
        "Flip-model presentation reduces input lag",
        "SwapEffectUpgrade enabled for windowed games",
        SAFE,
        true,
        true,
    ),
    t(
        "Disable Network Throttling",
        "Removes packet rate limits during gaming",
        "Network throttling index disabled",
        SAFE,
        true,
        true,
    ),
    t(
        "Optimize TCP Auto-Tuning",
        "Scales TCP receive windows dynamically",
        "TCP auto-tuning set to normal",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Game DVR & Background Recording",
        "Stops Xbox Game Bar background video recording",
        "Game DVR background capture disabled",
        MID,
        false,
        true,
    ),
    t(
        "Disable HPET (High Precision Event Timer)",
        "Forces low-latency hardware TSC timer for gaming",
        "HPET platform clock disabled",
        HIGH,
        false,
        true,
    ),

    // --- Privacy (36..43) ---
    t(
        "Disable Activity History",
        "Stops logging apps, files, and timeline",
        "Activity history and feed disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Tailored Experiences",
        "Stops diagnostic personalized ads/tips",
        "Tailored experiences disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Feedback Prompts",
        "Stops periodic feedback rating popups",
        "Feedback frequency set to Never",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable App-Launch Tracking",
        "Stops tracking launched apps for Start",
        "App launch tracking disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Cloud Clipboard Sync",
        "Keeps copied text/images local only",
        "Cross-device cloud clipboard disabled",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable P2P Update Sharing",
        "Stops uploading updates to internet peers",
        "Delivery optimization P2P upload off",
        SAFE,
        true,
        false,
    ),
    t(
        "Disable Diagnostic Telemetry & DiagTrack",
        "Stops Connected User Experiences and telemetry service",
        "DiagTrack service and telemetry disabled",
        MID,
        false,
        true,
    ),
];

fn mb(b: u64) -> String {
    format!("{} MB", b / 1_048_576)
}

/// The second line of a row. Uses real facts about this PC when they're known.
pub fn now_text(i: usize, f: Option<&scan::Facts>) -> String {
    let def = &TWEAKS[i];
    let f = match f {
        Some(f) => f,
        None => return def.now.to_string(),
    };
    match i {
        1 => {
            if f.hibernation {
                "Now: active (hiberfil.sys present)".to_string()
            } else {
                "Now: already off".to_string()
            }
        }
        2 => {
            if f.transparency {
                "Now: blur effects enabled".to_string()
            } else {
                "Now: already disabled".to_string()
            }
        }
        6 => match f.wsearch {
            Some(true) => "Now: running (indexing files in background)".to_string(),
            Some(false) => "Now: stopped".to_string(),
            None => def.now.to_string(),
        },
        7 => {
            if f.plan.to_lowercase().contains("high") {
                "Now: active scheme (CPMINCORES 100)".to_string()
            } else {
                format!("Now: active scheme {}", f.plan)
            }
        }
        19 => {
            if f.paging_exec_disabled {
                "Now: locked in physical RAM".to_string()
            } else {
                "Now: kernel pages to disk when needed".to_string()
            }
        }
        20 => match f.sysmain {
            Some(true) => "Now: running (caching programs in RAM)".to_string(),
            Some(false) => "Now: stopped".to_string(),
            None => def.now.to_string(),
        },
        22 => {
            if f.large_system_cache {
                "Now: large system cache active".to_string()
            } else {
                "Now: standard cache (recommended for games)".to_string()
            }
        }
        23 => {
            let total = f.user_temp + f.win_temp;
            if total > 0 {
                format!("{} found in temp folders", mb(total))
            } else {
                def.now.to_string()
            }
        }
        24 => {
            if f.update_cache > 0 {
                format!("{} found in update download cache", mb(f.update_cache))
            } else {
                def.now.to_string()
            }
        }
        25 => {
            if f.delivery_cache > 0 {
                format!("{} cached update fragments", mb(f.delivery_cache))
            } else {
                def.now.to_string()
            }
        }
        26 => {
            if f.browser_cache > 0 {
                format!("{} found across Chrome, Edge, Firefox", mb(f.browser_cache))
            } else {
                def.now.to_string()
            }
        }
        29 => {
            if f.crash_dumps > 0 {
                format!("{} found in crash dump files", mb(f.crash_dumps))
            } else {
                def.now.to_string()
            }
        }
        _ => def.now.to_string(),
    }
}

fn run(exe: &str, args: &[&str]) -> bool {
    Command::new(exe)
        .args(args)
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn sh(script: &str) -> bool {
    Command::new("cmd")
        .args(["/c", script])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ps(script: &str) -> bool {
    Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn reg(key: &str, name: &str, typ: &str, val: &str) -> bool {
    run("reg", &["add", key, "/v", name, "/t", typ, "/d", val, "/f"])
}

pub fn del_folder(path: &str) {
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let _ = std::fs::remove_dir_all(&p);
            } else {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
}

/// Applies one change. Returns true when it worked.
pub fn apply(idx: usize) -> bool {
    let win = scan::windows_dir();
    let mm = r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management";
    match idx {
        // --- Optimize ---
        0 => {
            reg(
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
                "GlobalUserDisabled",
                "REG_DWORD",
                "1",
            ) && reg(
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Search",
                "BackgroundAppGlobalToggle",
                "REG_DWORD",
                "0",
            )
        }
        1 => run("powercfg", &["/hibernate", "off"]),
        2 => reg(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "EnableTransparency",
            "REG_DWORD",
            "0",
        ),
        3 => {
            reg(
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
                "StartupDelayInMSec",
                "REG_DWORD",
                "0",
            ) && reg(
                r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
                "StartupDelayInMSec",
                "REG_DWORD",
                "0",
            )
        }
        4 => reg(r"HKCU\Control Panel\Desktop", "MenuShowDelay", "REG_SZ", "0"),
        5 => {
            sh("sc stop WerSvc & sc config WerSvc start= disabled")
                && reg(r"HKLM\SOFTWARE\Microsoft\Windows\Windows Error Reporting", "Disabled", "REG_DWORD", "1")
        }
        6 => sh("sc stop WSearch & sc config WSearch start= disabled"),

        // --- CPU ---
        7 => {
            sh("powercfg /setacvalueindex scheme_current sub_processor CPMINCORES 100")
                && sh("powercfg /setactive scheme_current")
        }
        8 => {
            sh("powercfg /setacvalueindex scheme_current sub_processor PERFBOOSTMODE 2")
                && sh("powercfg /setactive scheme_current")
        }
        9 => {
            sh("powercfg /setacvalueindex scheme_current sub_processor PERFEPP 10")
                && sh("powercfg /setactive scheme_current")
        }
        10 => {
            reg(
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games",
                "Priority",
                "REG_DWORD",
                "6",
            ) && reg(
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games",
                "Scheduling Category",
                "REG_SZ",
                "High",
            ) && reg(
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games",
                "SFIO Priority",
                "REG_SZ",
                "High",
            )
        }
        11 => {
            sh("powercfg /setacvalueindex scheme_current sub_processor PROCTHROTTLEMIN 100")
                && sh("powercfg /setactive scheme_current")
        }
        12 => {
            sh("powercfg /setacvalueindex scheme_current sub_pciExpress ASPM 0")
                && sh("powercfg /setactive scheme_current")
        }
        13 => run("bcdedit.exe", &["/set", "disabledynamictick", "yes"]),
        14 => {
            reg(mm, "FeatureSettingsOverride", "REG_DWORD", "3")
                && reg(mm, "FeatureSettingsOverrideMask", "REG_DWORD", "3")
        }

        // --- Memory ---
        15 => ps("Enable-MMAgent -MemoryCompression"),
        16 => {
            let ok_ps = ps("Enable-MMAgent -PageCombining");
            let ok_cmd = mem::combine_memory_pages();
            ok_ps || ok_cmd
        }
        17 => mem::clean([false, false, true, false])[2] == Some(true),
        18 => mem::clean([true, false, false, false])[0] == Some(true),
        19 => reg(mm, "DisablePagingExecutive", "REG_DWORD", "1"),
        20 => sh("sc stop SysMain & sc config SysMain start= disabled"),
        21 => ps("Disable-MMAgent -MemoryCompression"),
        22 => reg(mm, "LargeSystemCache", "REG_DWORD", "1"),

        // --- Cleanup ---
        23 => {
            del_folder(&std::env::temp_dir().to_string_lossy());
            del_folder(&format!(r"{}\Temp", win));
            true
        }
        24 => {
            sh(&format!(
                r"net stop wuauserv & rmdir /s /q {}\SoftwareDistribution\Download & mkdir {}\SoftwareDistribution\Download & net start wuauserv",
                win, win
            ))
        }
        25 => {
            del_folder(&format!(r"{}\SoftwareDistribution\DeliveryOptimization\Cache", win));
            sh(&format!(r"del /q /f /s {}\SoftwareDistribution\DeliveryOptimization\* 2>nul", win));
            true
        }
        26 => {
            if let Some(local) = scan::local_app_data() {
                del_folder(&local.join(r"Google\Chrome\User Data\Default\Cache").to_string_lossy());
                del_folder(&local.join(r"Google\Chrome\User Data\Default\Code Cache").to_string_lossy());
                del_folder(&local.join(r"Microsoft\Edge\User Data\Default\Cache").to_string_lossy());
                del_folder(&local.join(r"Microsoft\Edge\User Data\Default\Code Cache").to_string_lossy());
                if let Ok(rd) = std::fs::read_dir(local.join(r"Mozilla\Firefox\Profiles")) {
                    for entry in rd.flatten() {
                        let cache2 = entry.path().join("cache2");
                        if cache2.exists() {
                            del_folder(&cache2.to_string_lossy());
                        }
                    }
                }
            }
            true
        }
        27 => ps("Clear-RecycleBin -Force -ErrorAction SilentlyContinue"),
        28 => run("dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup"]),
        29 => {
            del_folder(&format!(r"{}\Minidump", win));
            del_folder(&format!(r"{}\LiveKernelReports", win));
            let _ = std::fs::remove_file(format!(r"{}\MEMORY.DMP", win));
            true
        }

        // --- Gaming ---
        30 => {
            reg(r"HKCU\Control Panel\Accessibility\StickyKeys", "Flags", "REG_SZ", "506")
                && reg(r"HKCU\Control Panel\Accessibility\ToggleKeys", "Flags", "REG_SZ", "58")
                && reg(r"HKCU\Control Panel\Accessibility\Keyboard Response", "Flags", "REG_SZ", "122")
        }
        31 => reg(
            r"HKCU\Software\Microsoft\DirectX\UserGlobalSettings",
            "SwapEffectUpgradeEnable",
            "REG_DWORD",
            "1",
        ),
        32 => {
            reg(
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
                "NetworkThrottlingIndex",
                "REG_DWORD",
                "4294967295",
            ) && reg(
                r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
                "SystemResponsiveness",
                "REG_DWORD",
                "0",
            )
        }
        33 => run("netsh", &["int", "tcp", "set", "global", "autotuninglevel=normal"]),
        34 => {
            reg(r"HKCU\System\GameConfigStore", "GameDVR_Enabled", "REG_DWORD", "0")
                && reg(r"HKCU\System\GameConfigStore", "GameDVR_FSEBehavior", "REG_DWORD", "2")
                && reg(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\GameDVR", "AllowGameDVR", "REG_DWORD", "0")
        }
        35 => {
            let _ = run("bcdedit.exe", &["/deletevalue", "useplatformclock"]);
            run("bcdedit.exe", &["/set", "useplatformtick", "yes"])
        }

        // --- Privacy ---
        36 => {
            reg(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System",
                "EnableActivityFeed",
                "REG_DWORD",
                "0",
            ) && reg(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System",
                "PublishUserActivities",
                "REG_DWORD",
                "0",
            ) && reg(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System",
                "UploadUserActivities",
                "REG_DWORD",
                "0",
            )
        }
        37 => {
            reg(
                r"HKCU\Software\Policies\Microsoft\Windows\CloudContent",
                "DisableTailoredExperiencesWithDiagnosticData",
                "REG_DWORD",
                "1",
            ) && reg(
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Privacy",
                "TailoredExperiencesWithDiagnosticDataEnabled",
                "REG_DWORD",
                "0",
            )
        }
        38 => {
            reg(r"HKCU\Software\Microsoft\Siuf\Rules", "NumberOfSIUFInPeriod", "REG_DWORD", "0")
                && reg(
                    r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection",
                    "DoNotShowFeedbackNotifications",
                    "REG_DWORD",
                    "1",
                )
        }
        39 => reg(
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced",
            "Start_TrackProgs",
            "REG_DWORD",
            "0",
        ),
        40 => {
            reg(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System",
                "AllowCrossDeviceClipboard",
                "REG_DWORD",
                "0",
            ) && reg(
                r"HKCU\Software\Microsoft\Clipboard",
                "EnableClipboardHistory",
                "REG_DWORD",
                "0",
            )
        }
        41 => {
            reg(
                r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization",
                "DODownloadMode",
                "REG_DWORD",
                "0",
            ) && reg(
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\DeliveryOptimization",
                "SystemSettingsDownloadMode",
                "REG_DWORD",
                "0",
            )
        }
        42 => {
            let _ = sh("sc stop DiagTrack & sc config DiagTrack start= disabled");
            reg(r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection", "AllowTelemetry", "REG_DWORD", "0")
        }
        _ => false,
    }
}
