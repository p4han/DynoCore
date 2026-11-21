//! Auto-updater for DynoCore using GitHub Releases and native WinINet API.
use std::fs;
use std::mem::size_of;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::null;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use windows_sys::Win32::Networking::WinInet::*;

const REPO_API: &str = "https://api.github.com/repos/p4han/DynoCore/releases/latest";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub static UPDATE_CHECK_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct ReleaseInfo {
    pub version: String,
    #[allow(dead_code)]
    pub tag: String,
    pub download_url: String,
    pub is_installer: bool,
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Parses semantic version string (e.g. "1.0.1" or "v1.0.1") into (major, minor, patch)
fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let s = v.trim().trim_start_matches(|c| c == 'v' || c == 'V');
    let mut parts = s.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u32>().ok()?;
    Some((major, minor, patch))
}

pub fn is_newer_version(remote: &str, current: &str) -> bool {
    if let (Some(r), Some(c)) = (parse_version(remote), parse_version(current)) {
        r > c
    } else {
        false
    }
}

/// Simple JSON string value extractor for a given key
fn json_find_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{}\"", key);
    let key_idx = json.find(&pattern)?;
    let after_key = &json[key_idx + pattern.len()..];
    let colon_idx = after_key.find(':')?;
    let val_part = after_key[colon_idx + 1..].trim_start();
    if val_part.starts_with('"') {
        let inside = &val_part[1..];
        let end_quote = inside.find('"')?;
        Some(&inside[..end_quote])
    } else {
        None
    }
}

/// Performs a GET request using native WinINet and returns the response body as bytes
pub fn http_get(url: &str) -> Result<Vec<u8>, String> {
    unsafe {
        let agent = wide("DynoCore-Updater");
        let h_inet = InternetOpenW(
            agent.as_ptr(),
            INTERNET_OPEN_TYPE_PRECONFIG,
            null(),
            null(),
            0,
        );
        if h_inet.is_null() {
            return Err("InternetOpenW failed".to_string());
        }

        let url_w = wide(url);
        let headers = wide("User-Agent: DynoCore-Updater\r\nAccept: application/vnd.github.v3+json\r\n");
        let flags = INTERNET_FLAG_RELOAD
            | INTERNET_FLAG_SECURE
            | INTERNET_FLAG_DONT_CACHE
            | INTERNET_FLAG_IGNORE_CERT_CN_INVALID
            | INTERNET_FLAG_IGNORE_CERT_DATE_INVALID;

        let h_file = InternetOpenUrlW(
            h_inet,
            url_w.as_ptr(),
            headers.as_ptr(),
            headers.len() as u32,
            flags,
            0,
        );

        if h_file.is_null() {
            InternetCloseHandle(h_inet);
            return Err("InternetOpenUrlW failed".to_string());
        }

        let mut data = Vec::new();
        let mut buf = [0u8; 8192];
        let mut read = 0u32;

        loop {
            if InternetReadFile(h_file, buf.as_mut_ptr() as _, buf.len() as u32, &mut read) == 0 {
                break;
            }
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buf[..read as usize]);
        }

        InternetCloseHandle(h_file);
        InternetCloseHandle(h_inet);

        Ok(data)
    }
}

/// Checks GitHub releases API for latest version
pub fn check_for_update() -> Result<Option<ReleaseInfo>, String> {
    let bytes = http_get(REPO_API)?;
    let text = String::from_utf8_lossy(&bytes);

    let tag = match json_find_string(&text, "tag_name") {
        Some(t) => t.to_string(),
        None => return Err("No tag_name in GitHub release".to_string()),
    };

    let remote_ver = tag.trim_start_matches(|c| c == 'v' || c == 'V').to_string();
    if !is_newer_version(&remote_ver, CURRENT_VERSION) {
        return Ok(None);
    }

    // Look for assets: prefer dyno.exe for seamless binary swap, or Dyno-Setup-x64.exe
    let mut download_url = String::new();
    let mut is_installer = false;

    // Search for assets array in JSON
    if let Some(assets_start) = text.find("\"assets\"") {
        let assets_text = &text[assets_start..];
        // Split by "browser_download_url"
        for chunk in assets_text.split("\"browser_download_url\"") {
            if let Some(colon) = chunk.find(':') {
                let rest = chunk[colon + 1..].trim_start();
                if rest.starts_with('"') {
                    if let Some(end) = rest[1..].find('"') {
                        let url = &rest[1..1 + end];
                        if url.ends_with("/dyno.exe") {
                            download_url = url.to_string();
                            is_installer = false;
                            break;
                        } else if url.ends_with(".exe") && download_url.is_empty() {
                            download_url = url.to_string();
                            is_installer = true;
                        }
                    }
                }
            }
        }
    }

    if download_url.is_empty() {
        return Err("No downloadable .exe asset found in release".to_string());
    }

    Ok(Some(ReleaseInfo {
        version: remote_ver,
        tag,
        download_url,
        is_installer,
    }))
}

/// Streams download from URL with progress reporting to a temporary file
pub fn download_update<F>(url: &str, mut progress: F) -> Result<PathBuf, String>
where
    F: FnMut(f32, u64, u64),
{
    unsafe {
        let agent = wide("DynoCore-Updater");
        let h_inet = InternetOpenW(
            agent.as_ptr(),
            INTERNET_OPEN_TYPE_PRECONFIG,
            null(),
            null(),
            0,
        );
        if h_inet.is_null() {
            return Err("InternetOpenW failed".to_string());
        }

        let url_w = wide(url);
        let headers = wide("User-Agent: DynoCore-Updater\r\n");
        let flags = INTERNET_FLAG_RELOAD
            | INTERNET_FLAG_SECURE
            | INTERNET_FLAG_DONT_CACHE
            | INTERNET_FLAG_IGNORE_CERT_CN_INVALID
            | INTERNET_FLAG_IGNORE_CERT_DATE_INVALID;

        let h_file = InternetOpenUrlW(
            h_inet,
            url_w.as_ptr(),
            headers.as_ptr(),
            headers.len() as u32,
            flags,
            0,
        );

        if h_file.is_null() {
            InternetCloseHandle(h_inet);
            return Err("InternetOpenUrlW failed for download".to_string());
        }

        // Try getting Content-Length
        let mut total_bytes: u32 = 0;
        let mut len_size = size_of::<u32>() as u32;
        let mut index = 0u32;
        let has_len = HttpQueryInfoW(
            h_file,
            HTTP_QUERY_CONTENT_LENGTH | HTTP_QUERY_FLAG_NUMBER,
            &mut total_bytes as *mut u32 as *mut _,
            &mut len_size,
            &mut index,
        ) != 0;

        let total = if has_len && total_bytes > 0 { total_bytes as u64 } else { 0 };

        let temp_dir = std::env::temp_dir();
        let target_path = temp_dir.join(format!("dyno_update_{}.exe", std::process::id()));

        let mut file = fs::File::create(&target_path)
            .map_err(|e| format!("Failed to create temp file: {}", e))?;

        use std::io::Write;

        let mut downloaded: u64 = 0;
        let mut buf = [0u8; 16384];
        let mut read = 0u32;

        loop {
            if InternetReadFile(h_file, buf.as_mut_ptr() as _, buf.len() as u32, &mut read) == 0 {
                break;
            }
            if read == 0 {
                break;
            }
            file.write_all(&buf[..read as usize])
                .map_err(|e| format!("Failed writing update data: {}", e))?;
            downloaded += read as u64;

            let pct = if total > 0 {
                (downloaded as f32 / total as f32 * 100.0).clamp(0.0, 99.0)
            } else {
                50.0
            };
            progress(pct, downloaded, total);
        }

        drop(file);
        InternetCloseHandle(h_file);
        InternetCloseHandle(h_inet);

        if downloaded == 0 {
            let _ = fs::remove_file(&target_path);
            return Err("Download finished with 0 bytes".to_string());
        }

        progress(100.0, downloaded, downloaded);
        Ok(target_path)
    }
}

/// Applies the downloaded update and relaunches the application
pub fn apply_update_and_restart(update_file: &Path, is_installer: bool) -> Result<(), String> {
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;

    if is_installer {
        // Run installer silently to update in place
        let _ = Command::new(update_file)
            .args(&["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"])
            .creation_flags(0x08000000)
            .spawn();
        std::thread::sleep(Duration::from_millis(500));
        std::process::exit(0);
    }

    let exe_dir = current_exe.parent().ok_or("No parent dir")?;
    let exe_old = exe_dir.join("dyno.exe.old");
    let exe_staged = exe_dir.join("dyno.exe.new");

    // Clean up any stale old/staged files
    let _ = fs::remove_file(&exe_old);
    let _ = fs::remove_file(&exe_staged);

    // Copy temp downloaded file into same directory as current exe
    fs::copy(update_file, &exe_staged).map_err(|e| format!("Copy staged failed: {}", e))?;

    // Rename current running exe -> dyno.exe.old (NTFS allows renaming running binaries)
    if let Err(e) = fs::rename(&current_exe, &exe_old) {
        let _ = fs::remove_file(&exe_staged);
        return Err(format!("Rename current failed: {}", e));
    }

    // Move dyno.exe.new -> dyno.exe
    if let Err(e) = fs::rename(&exe_staged, &current_exe) {
        // Try restoring original
        let _ = fs::rename(&exe_old, &current_exe);
        return Err(format!("Move staged failed: {}", e));
    }

    // Cleanup temp download
    let _ = fs::remove_file(update_file);

    // Spawn new version with --update-cleanup
    let _ = Command::new(&current_exe)
        .arg("--update-cleanup")
        .spawn();

    std::process::exit(0);
}

/// Cleans up any leftover .old binary from a previous update
pub fn cleanup_previous_update() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_millis(1500));
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(dir) = current_exe.parent() {
                let exe_old = dir.join("dyno.exe.old");
                if exe_old.exists() {
                    let _ = fs::remove_file(exe_old);
                }
            }
        }
    });
}
