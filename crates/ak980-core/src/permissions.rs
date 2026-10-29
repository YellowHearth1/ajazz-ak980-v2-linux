//! Access to the keyboard without root: the udev rule.

use std::ffi::CString;
use std::fs;
use std::path::Path;

use crate::Result;
use crate::device::{DONGLE_PID, VENDOR_ID, WIRED_PID};
use crate::hid::enumerate;
use crate::service::{run_as_root, shell_quote};

pub const RULE_NAME: &str = "60-ajazz-ak980.rules";
pub const RULE: &str = include_str!("../../../ak980/data/60-ajazz-ak980.rules");
pub const ETC_RULE: &str = "/etc/udev/rules.d/60-ajazz-ak980.rules";
pub const RULE_LOCATIONS: [&str; 3] =
    ["/usr/lib/udev/rules.d/60-ajazz-ak980.rules", ETC_RULE, "/lib/udev/rules.d/60-ajazz-ak980.rules"];

/// The interfaces the app opens (see the rule); the others carry keystrokes and stay closed to users.
pub const APP_INTERFACES: [(u16, &[i32]); 2] = [(WIRED_PID, &[2, 3]), (DONGLE_PID, &[3, 4])];

fn app_interfaces(product_id: u16) -> &'static [i32] {
    APP_INTERFACES.iter().find(|(pid, _)| *pid == product_id).map(|(_, ifs)| *ifs).unwrap_or(&[])
}

pub fn rule_installed() -> bool {
    RULE_LOCATIONS.iter().any(|p| Path::new(p).exists())
}

/// A distribution package ships the rule; a copy in /etc would shadow its future updates.
pub fn rule_packaged() -> bool {
    RULE_LOCATIONS.iter().filter(|p| **p != ETC_RULE).any(|p| Path::new(p).exists())
}

/// udev reads a copy in /etc that is not this version's rule, or that hides the packaged one.
///
/// 0.0.1 wrote such a copy without a package, and its rule also opened the keystroke interfaces.
pub fn rule_outdated() -> bool {
    rule_outdated_at(Path::new(ETC_RULE), rule_packaged())
}

pub fn rule_outdated_at(etc_copy: &Path, packaged: bool) -> bool {
    match fs::read_to_string(etc_copy) {
        Ok(copy) => packaged || copy != RULE,
        Err(_) => false,
    }
}

/// Every hidraw node of the keyboard and its dongle.
pub fn keyboard_nodes() -> Vec<String> {
    enumerate(Some(VENDOR_ID), None)
        .into_iter()
        .filter(|d| !app_interfaces(d.product_id).is_empty())
        .map(|d| d.path)
        .collect()
}

/// The nodes the app needs: the rule opens these and only these.
pub fn app_nodes() -> Vec<String> {
    enumerate(Some(VENDOR_ID), None)
        .into_iter()
        .filter(|d| app_interfaces(d.product_id).contains(&d.interface))
        .map(|d| d.path)
        .collect()
}

fn read_write(path: &str) -> bool {
    let Ok(c) = CString::new(path) else { return false };
    // SAFETY: access only reads the path we pass
    unsafe { libc::access(c.as_ptr(), libc::R_OK | libc::W_OK) == 0 }
}

/// Some(true/false) for the nodes the app needs, None when no keyboard is connected.
pub fn accessible() -> Option<bool> {
    let nodes = app_nodes();
    if nodes.is_empty() {
        return None;
    }
    Some(nodes.iter().all(|p| read_write(p)))
}

/// The root script `install_rule` runs. It makes this version's rule the one udev reads (a copy in
/// /etc unless a package ships the rule; with a package, a copy left in /etc goes away), reloads
/// the rules and sends the keyboard's nodes a "change" event, which also closes the keystroke
/// interfaces an older rule opened.
pub fn install_script(packaged: bool, etc_copy: bool, nodes: &[String]) -> String {
    let mut script = String::from("set -e\n");
    if !packaged {
        script += "mkdir -p /etc/udev/rules.d\n";
        script += &format!("cat > {ETC_RULE} <<'AK980_EOF'\n{RULE}AK980_EOF\n");
        script += &format!("chmod 644 {ETC_RULE}\n");
    } else if etc_copy {
        script += &format!("rm -f {ETC_RULE}\n");
    }
    script += "udevadm control --reload-rules\n";
    script += &trigger_command(nodes);
    script
}

/// Installs or repairs the rule and applies it to the connected keyboard. Asks for the user's
/// password through pkexec.
pub fn install_rule() -> Result<()> {
    run_as_root(&install_script(rule_packaged(), Path::new(ETC_RULE).exists(), &keyboard_nodes()))
}

/// Re-applies the rule to the keyboard's own nodes, with "change" as udev's reload hooks do.
///
/// Every node of the keyboard, so that the keystroke interfaces lose what an older rule gave them.
/// An "add" for every hidraw device would tell other programs their devices were plugged in again.
pub fn trigger_command(nodes: &[String]) -> String {
    if nodes.is_empty() {
        return String::new(); // nothing connected: the rule applies when the keyboard is plugged in
    }
    let names: Vec<String> = nodes
        .iter()
        .map(|n| {
            let name = Path::new(n).file_name().and_then(|f| f.to_str()).unwrap_or(n);
            format!("--sysname-match={}", shell_quote(name))
        })
        .collect();
    format!("udevadm trigger --action=change --subsystem-match=hidraw {}\n", names.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn rule_lines() -> Vec<&'static str> {
        RULE.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
    }

    #[test]
    fn trigger_targets_only_the_keyboard() {
        assert_eq!(trigger_command(&[]), "");
        assert_eq!(
            trigger_command(&v(&["/dev/hidraw0", "/dev/hidraw7"])),
            "udevadm trigger --action=change --subsystem-match=hidraw --sysname-match=hidraw0 --sysname-match=hidraw7\n"
        );
    }

    #[test]
    fn rule_opens_only_the_vendor_interfaces() {
        let lines = rule_lines();
        let others = lines.iter().position(|l| *l == r#"ENV{.AJAZZ_AK980}=="", GOTO="ajazz_ak980_end""#).unwrap();
        let closed = lines.iter().position(|l| *l == r#"MODE="0600", GROUP="root""#).unwrap();
        let opened: Vec<usize> = (0..lines.len()).filter(|i| lines[*i].contains("uaccess")).collect();
        assert!(others < closed && closed < opened[0] && opened.len() == 2);
        assert!(lines[opened[0]].contains(r#"ENV{.AJAZZ_AK980}=="usb", ATTRS{bInterfaceNumber}=="02|03""#));
        assert!(lines[opened[1]].contains(r#"ENV{.AJAZZ_AK980}=="dongle", ATTRS{bInterfaceNumber}=="03|04""#));
    }

    #[test]
    fn access_check_follows_the_rule() {
        for (pid, kind) in [(WIRED_PID, "usb"), (DONGLE_PID, "dongle")] {
            let want: Vec<String> = app_interfaces(pid).iter().map(|i| format!("{i:02}")).collect();
            let line = format!(r#"ENV{{.AJAZZ_AK980}}=="{kind}", ATTRS{{bInterfaceNumber}}=="{}""#, want.join("|"));
            assert!(RULE.contains(&line), "{line}");
        }
    }

    #[test]
    fn outdated_copies() {
        let dir = std::env::temp_dir().join(format!("ak980-rule-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let etc = dir.join(RULE_NAME);
        assert!(!rule_outdated_at(&etc, false), "no copy in /etc");
        fs::write(&etc, RULE).unwrap();
        assert!(!rule_outdated_at(&etc, false), "this version's rule");
        assert!(rule_outdated_at(&etc, true), "current, but it hides the package's");
        fs::write(&etc, "SUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"0c45\", MODE=\"0660\", TAG+=\"uaccess\"\n").unwrap();
        assert!(rule_outdated_at(&etc, false), "like 0.0.1's: every interface");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_script_shape() {
        let nodes = v(&["/dev/hidraw3", "/dev/hidraw7"]);
        let plain = install_script(false, false, &nodes);
        assert!(plain.contains(&format!("cat > {ETC_RULE} <<")) && plain.contains(r#"ATTRS{idVendor}=="0c45""#));
        // every node of the keyboard, so that the rule closes its keystroke interfaces again
        assert!(plain.ends_with("--sysname-match=hidraw3 --sysname-match=hidraw7\n") && !plain.contains("--action=add"));
        let packaged = install_script(true, false, &nodes);
        assert!(!packaged.contains("/etc/udev") && packaged.contains("udevadm control --reload-rules"));
        let leftover = install_script(true, true, &nodes);
        assert!(leftover.starts_with(&format!("set -e\nrm -f {ETC_RULE}\nudevadm control --reload-rules\n")));
    }
}
