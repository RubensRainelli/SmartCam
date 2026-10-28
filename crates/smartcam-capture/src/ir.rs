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

/// Detects whether a camera device name or symbolic link indicates an Infrared/depth/biometric sensor
/// (e.g. Windows Hello, Howdy, IR dot projector, grey-only sensor).
pub fn is_ir_camera_name(name: &str, symlink: &str) -> bool {
    let n_up = name.to_ascii_uppercase();
    let s_up = symlink.to_ascii_uppercase();
    if n_up.contains("INFRARED")
        || n_up.contains("INFRAROSSI")
        || n_up.contains("WINDOWS HELLO")
        || s_up.contains("INFRARED")
        || s_up.contains("WINDOWS HELLO")
        || s_up.contains("SENSOR_CAMERA")
    {
        return true;
    }
    has_ir_word(&n_up) || has_ir_word(&s_up)
}

fn has_ir_word(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    for i in 0..=(bytes.len() - 2) {
        if bytes[i] == b'I' && bytes[i + 1] == b'R' {
            let left_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let right_ok = (i + 2 >= bytes.len()) || !bytes[i + 2].is_ascii_alphabetic();
            if left_ok && right_ok {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ir_detection() {
        assert!(is_ir_camera_name("Integrated IR Camera", ""));
        assert!(is_ir_camera_name("Camera Infrarossi", ""));
        assert!(is_ir_camera_name("Windows Hello Face Software Device", ""));
        assert!(is_ir_camera_name("RGB-IR Sensor", ""));
        assert!(is_ir_camera_name("Webcam", r"\\?\usb#vid_04f2&pid_b6d9&mi_02#...sensor_camera..."));
        assert!(!is_ir_camera_name("Logitech Brio 4K", r"\\?\usb#vid_046d..."));
        assert!(!is_ir_camera_name("Integrated Camera", "usb-0000:00:14.0-8"));
    }
}
