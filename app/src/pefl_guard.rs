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

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn pefl_cache_candidates(repo_root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.push(repo_root.join(".pefl_accepted_SmartCam"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(".pefl_accepted_SmartCam");
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    paths
}

pub fn is_pefl_accepted(repo_root: &Path) -> bool {
    let now = current_unix_timestamp();
    for path in pefl_cache_candidates(repo_root) {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(ts) = content.trim().parse::<u64>() {
                if now >= ts && (now - ts) < SEVEN_DAYS_SECONDS {
                    return true;
                }
            }
        }
    }
    false
}

pub fn save_pefl_acceptance(repo_root: &Path, remember_7_days: bool) {
    let now_str = current_unix_timestamp().to_string();
    // Always remove any legacy .exe-suffixed cache file if present
    let _ = fs::remove_file(repo_root.join(".pefl_accepted_SmartCam.exe"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = fs::remove_file(dir.join(".pefl_accepted_SmartCam.exe"));
        }
    }

    for path in pefl_cache_candidates(repo_root) {
        if remember_7_days {
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
