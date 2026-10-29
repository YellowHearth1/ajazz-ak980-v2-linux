//! Access to the keyboard without root: the udev rule.

use std::ffi::CString;
use std::path::Path;

use crate::Result;
use crate::device::{DONGLE_PID, VENDOR_ID, WIRED_PID};
use crate::hid::enumerate;
use crate::service::{run_as_root, shell_quote};

pub const RULE_NAME: &str = "60-ajazz-ak980.rules";
pub const RULE: &str = include_str!("../../../ak980/data/60-ajazz-ak980.rules");
pub const RULE_LOCATIONS: [&str; 3] = [
    "/usr/lib/udev/rules.d/60-ajazz-ak980.rules",
    "/etc/udev/rules.d/60-ajazz-ak980.rules",
    "/lib/udev/rules.d/60-ajazz-ak980.rules",
];

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
    RULE_LOCATIONS.iter().filter(|p| !p.starts_with("/etc/")).any(|p| Path::new(p).exists())
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

/// The root script `install_rule` runs: the rule into /etc unless a package ships it, a reload, a
/// "change" event for the keyboard's nodes, and the keystroke nodes back to root only.
pub fn install_script(packaged: bool, nodes: &[String], keep: &[String]) -> String {
    let mut script = String::from("set -e\n");
    if !packaged {
        script += "mkdir -p /etc/udev/rules.d\n";
        script += &format!("cat > /etc/udev/rules.d/{RULE_NAME} <<'AK980_EOF'\n{RULE}AK980_EOF\n");
        script += &format!("chmod 644 /etc/udev/rules.d/{RULE_NAME}\n");
    }
    script += "udevadm control --reload-rules\n";
    script += &trigger_command(nodes);
    script += &revoke_command(nodes, keep);
    script
}

/// Puts the rule in place unless a package ships it, then applies it to the connected keyboard.
/// Asks for the user's password through pkexec.
pub fn install_rule() -> Result<()> {
    run_as_root(&install_script(rule_packaged(), &keyboard_nodes(), &app_nodes()))
}

/// Puts the keystroke interfaces back the way the kernel creates them (root only, no ACL).
///
/// The rule of 0.0.1 opened them too, and a node that no longer matches keeps its old group, mode
/// and ACL until it is plugged in again.
pub fn revoke_command(nodes: &[String], keep: &[String]) -> String {
    let stale: Vec<String> = nodes.iter().filter(|n| !keep.contains(n)).map(|n| shell_quote(n)).collect();
    if stale.is_empty() {
        return String::new();
    }
    let stale = stale.join(" ");
    format!(
        "udevadm settle --timeout=5 || true\nchown root:root {stale}\nchmod 0600 {stale}\n\
         if command -v setfacl >/dev/null; then setfacl -b {stale}; fi\n"
    )
}

/// Re-applies the rule to the keyboard's own nodes, with "change" as udev's reload hooks do.
///
/// An "add" for every hidraw device would tell other programs their devices were plugged in again.
pub fn trigger_command(nodes: &[String]) -> String {
    if nodes.is_empty() {
        return String::new(); // nothing connected: the rule applies when the keyboard is plugged in
    }
    let names: Vec<String> = nodes
        .iter()
        .map(|n| format!("--sysname-match={}", shell_quote(Path::new(n).file_name().map_or(n.as_str(), |f| f.to_str().unwrap_or(n)))))
        .collect();
    format!("udevadm trigger --action=change --subsystem-match=hidraw {}\n", names.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
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
    fn revoke_only_the_keystroke_nodes() {
        assert_eq!(revoke_command(&v(&["/dev/hidraw3"]), &v(&["/dev/hidraw3"])), "");
        let cmd = revoke_command(&v(&["/dev/hidraw7", "/dev/hidraw10"]), &v(&["/dev/hidraw10"]));
        assert!(cmd.contains("chmod 0600 /dev/hidraw7\n") && cmd.ends_with("setfacl -b /dev/hidraw7; fi\n"));
        assert!(!cmd.contains("hidraw10"));
    }

    #[test]
    fn rule_opens_only_the_vendor_interfaces() {
        let rules: Vec<&str> = RULE.lines().filter(|l| l.contains("uaccess") && !l.starts_with('#')).collect();
        assert_eq!(rules.len(), 2);
        assert!(rules[0].contains(r#"ENV{.AJAZZ_AK980}=="usb", ATTRS{bInterfaceNumber}=="02|03""#));
        assert!(rules[1].contains(r#"ENV{.AJAZZ_AK980}=="dongle", ATTRS{bInterfaceNumber}=="03|04""#));
    }

    #[test]
    fn install_script_shape() {
        let nodes = v(&["/dev/hidraw3"]);
        let plain = install_script(false, &nodes, &nodes);
        assert!(plain.contains("cat > /etc/udev/rules.d/") && plain.contains(r#"ATTRS{idVendor}=="0c45""#));
        assert!(plain.ends_with("--sysname-match=hidraw3\n") && !plain.contains("--action=add"));
        let packaged = install_script(true, &nodes, &nodes);
        assert!(!packaged.contains("/etc/udev") && packaged.contains("udevadm control --reload-rules"));
        let revoking = install_script(true, &v(&["/dev/hidraw7", "/dev/hidraw10"]), &v(&["/dev/hidraw10"]));
        assert!(revoking.contains("chmod 0600 /dev/hidraw7"));
    }
}
