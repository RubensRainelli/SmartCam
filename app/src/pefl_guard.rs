/*
 * Personal and Educational Free License (PEF License / PEFL)
 * Version 1.0 - 2026-05-26
 * Copyright (c) 2026 Rubens Rainelli
 *
 * Contact Details:
 *   Email: rubens@rainelli.it
 *   GitHub: https://github.com/RubensRainelli/
 *   Tel: (+39) 342 811 5882
 *
 * Personal and educational free use permitted.
 * Corporate, business or enterprise use prohibited without written authorization and fee.
 * The full license can be found in the LICENSE file.
 */

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SEVEN_DAYS_SECONDS: u64 = 7 * 24 * 60 * 60;

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static TEST_PEFL_STORE_OVERRIDE: Mutex<Option<(PathBuf, Option<u64>)>> = Mutex::new(None);

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, p: PathBuf) {
    if !paths.contains(&p) {
        paths.push(p);
    }
}

pub fn pefl_cache_candidates(repo_root: &Path) -> Vec<PathBuf> {
    #[cfg(test)]
    if let Ok(guard) = TEST_PEFL_STORE_OVERRIDE.lock() {
        if let Some((ref test_dir, _)) = *guard {
            return vec![
                test_dir.join(".pefl_accepted_SmartCam"),
                repo_root.join(".pefl_accepted_SmartCam"),
            ];
        }
    }

    let mut paths = Vec::new();

    if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
        let dir = PathBuf::from(local_appdata).join("SmartCam");
        push_unique_path(&mut paths, dir.join(".pefl_accepted_SmartCam"));
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("SmartCam");
        push_unique_path(&mut paths, dir.join(".pefl_accepted_SmartCam"));
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            let dir = PathBuf::from(xdg).join("SmartCam");
            push_unique_path(&mut paths, dir.join(".pefl_accepted_SmartCam"));
        } else if let Ok(home) = std::env::var("HOME") {
            let dir = PathBuf::from(home).join(".config").join("SmartCam");
            push_unique_path(&mut paths, dir.join(".pefl_accepted_SmartCam"));
        }
    }

    push_unique_path(&mut paths, repo_root.join(".pefl_accepted_SmartCam"));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push_unique_path(&mut paths, dir.join(".pefl_accepted_SmartCam"));
        }
    }

    paths
}

#[cfg(target_os = "windows")]
fn read_registry_pefl_timestamp() -> Option<u64> {
    #[cfg(test)]
    if let Ok(guard) = TEST_PEFL_STORE_OVERRIDE.lock() {
        if let Some((_, ts_opt)) = *guard {
            return ts_opt;
        }
    }

    use crate::win32_ffi::{
        to_wstring, RegCloseKey, RegOpenKeyExW, RegQueryValueExW, DWORD, HKEY, HKEY_CURRENT_USER,
        KEY_READ, NULL, REG_SZ,
    };

    unsafe {
        let subkey = to_wstring(r"Software\SmartCam");
        let val_name = to_wstring("PeflAcceptedTimestamp");
        let mut hkey: HKEY = NULL;
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0
            || hkey.is_null()
        {
            return None;
        }
        let mut buf = [0u16; 64];
        let mut buf_bytes: DWORD = (buf.len() * 2) as DWORD;
        let mut reg_type: DWORD = 0;
        let status = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            std::ptr::null_mut(),
            &mut reg_type,
            buf.as_mut_ptr() as *mut u8,
            &mut buf_bytes,
        );
        RegCloseKey(hkey);
        if status == 0 && reg_type == REG_SZ && buf_bytes >= 2 {
            let len = (buf_bytes as usize / 2).saturating_sub(1);
            let s = String::from_utf16_lossy(&buf[..len]);
            return s.trim_matches('\0').trim().parse::<u64>().ok();
        }
        None
    }
}

#[cfg(target_os = "windows")]
fn write_registry_pefl_timestamp(timestamp_opt: Option<u64>) {
    #[cfg(test)]
    if let Ok(mut guard) = TEST_PEFL_STORE_OVERRIDE.lock() {
        if let Some((_, ref mut ts_slot)) = *guard {
            *ts_slot = timestamp_opt;
            return;
        }
    }

    use crate::win32_ffi::{
        to_wstring, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, DWORD, HKEY,
        HKEY_CURRENT_USER, KEY_WRITE, NULL, REG_SZ,
    };

    unsafe {
        let subkey = to_wstring(r"Software\SmartCam");
        let val_name = to_wstring("PeflAcceptedTimestamp");
        let mut hkey: HKEY = NULL;
        let mut disp: DWORD = 0;
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            std::ptr::null_mut(),
            0,
            KEY_WRITE,
            std::ptr::null_mut(),
            &mut hkey,
            &mut disp,
        ) != 0
            || hkey.is_null()
        {
            return;
        }

        if let Some(ts) = timestamp_opt {
            let s = ts.to_string();
            let wide = to_wstring(&s);
            let _ = RegSetValueExW(
                hkey,
                val_name.as_ptr(),
                0,
                REG_SZ,
                wide.as_ptr() as *const u8,
                (wide.len() * 2) as DWORD,
            );
        } else {
            let _ = RegDeleteValueW(hkey, val_name.as_ptr());
        }
        RegCloseKey(hkey);
    }
}

fn is_timestamp_valid(ts: u64, now: u64) -> bool {
    if now >= ts {
        (now - ts) < SEVEN_DAYS_SECONDS
    } else {
        // Tolerate minor system clock adjustments
        (ts - now) < 3600
    }
}

pub fn is_pefl_accepted(repo_root: &Path) -> bool {
    let now = current_unix_timestamp();

    #[cfg(target_os = "windows")]
    if let Some(ts) = read_registry_pefl_timestamp() {
        if is_timestamp_valid(ts, now) {
            return true;
        }
    }

    for path in pefl_cache_candidates(repo_root) {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(ts) = content.trim().parse::<u64>() {
                if is_timestamp_valid(ts, now) {
                    return true;
                }
            }
        }
    }
    false
}

pub fn save_pefl_acceptance(repo_root: &Path, remember_7_days: bool) {
    let now = current_unix_timestamp();
    let now_str = now.to_string();

    // Always remove any legacy .exe-suffixed or shared Public cache file if present
    let _ = fs::remove_file(repo_root.join(".pefl_accepted_SmartCam.exe"));
    #[cfg(target_os = "windows")]
    #[cfg(not(test))]
    let _ = fs::remove_file(r"C:\Users\Public\SmartCam\.pefl_accepted_SmartCam");

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = fs::remove_file(dir.join(".pefl_accepted_SmartCam.exe"));
        }
    }

    #[cfg(target_os = "windows")]
    write_registry_pefl_timestamp(if remember_7_days { Some(now) } else { None });

    for path in pefl_cache_candidates(repo_root) {
        if remember_7_days {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, &now_str);
        } else {
            let _ = fs::remove_file(&path);
        }
    }
}

pub fn check_pefl_cli(repo_root: &Path) -> bool {
    if is_pefl_accepted(repo_root) {
        return true;
    }

    println!("============================================================");
    println!("        Personal and Educational Free License (PEFL) v1.0");
    println!("============================================================");
    println!("Copyright (c) 2026 Rubens Rainelli");
    println!("Official Specification: https://github.com/RubensRainelli/pefl-license-spec");
    println!("Author Contact: rubens@rainelli.it | Tel: (+39) 342 811 5882\n");
    println!("TERMINI D'USO / TERMS OF USE:");
    println!("  * GRATUITO per uso Personale ed Educativo (FREE for Personal & Educational use).");
    println!("  * SEVERAMENTE VIETATO l'uso Aziendale, Professionale o Commerciale");
    println!("    senza autorizzazione scritta e pagamento del corrispettivo.");
    println!("  * Esclude luoghi di culto e organizzazioni religiose (uso corporate).\n");
    println!("============================================================");

    print!("Accetti i termini della licenza PEFL v1.0? / Do you accept PEFL v1.0? (y/N): ");
    let _ = io::stdout().flush();
    let mut choice = String::new();
    if io::stdin().read_line(&mut choice).is_err() {
        return false;
    }
    let c = choice.trim().to_ascii_lowercase();
    if c != "y" && c != "yes" && c != "s" && c != "si" {
        eprintln!("[SmartCam] Licenza PEFL v1.0 rifiutata. Chiusura.");
        return false;
    }

    print!("Ricordare l'approvazione per i prossimi 7 giorni? (y/N): ");
    let _ = io::stdout().flush();
    let mut rem = String::new();
    let _ = io::stdin().read_line(&mut rem);
    let r = rem.trim().to_ascii_lowercase();
    let remember = r == "y" || r == "yes" || r == "s" || r == "si";
    save_pefl_acceptance(repo_root, remember);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pefl_acceptance_persists_across_readonly_or_custom_dirs() {
        let test_dir =
            std::env::temp_dir().join(format!("smartcam_pefl_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&test_dir);
        if let Ok(mut guard) = TEST_PEFL_STORE_OVERRIDE.lock() {
            *guard = Some((test_dir.clone(), None));
        }

        // Simulate a directory path that does not exist / cannot be written directly
        let non_writable_root = test_dir
            .join("non_existent_install_dir")
            .join("deep_readonly");
        assert!(!is_pefl_accepted(&non_writable_root));

        save_pefl_acceptance(&non_writable_root, true);
        assert!(
            is_pefl_accepted(&non_writable_root),
            "PEFL 7-day acceptance must persist via user-writable AppData/Registry even when install dir is read-only"
        );

        save_pefl_acceptance(&non_writable_root, false);
        assert!(
            !is_pefl_accepted(&non_writable_root),
            "Clearing PEFL acceptance must reset state to false"
        );

        if let Ok(mut guard) = TEST_PEFL_STORE_OVERRIDE.lock() {
            *guard = None;
        }
        let _ = fs::remove_dir_all(&test_dir);
    }
}
