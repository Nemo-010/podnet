//! Checks of the installer core in scripts/install.sh: a second run, the
//! downloaders that only podnet knows, PODNET_NO_VERIFY, and
//! scripts/install-test.sh in each shell, with a planted defect.

use super::*;

/// The inode and the modification time of FILE.
fn stamp(path: &Path) -> (u64, i64) {
    use std::os::unix::fs::MetadataExt as _;
    let m = std::fs::metadata(path).unwrap();
    (m.ino(), m.mtime_nsec() + m.mtime() * 1_000_000_000)
}

// A second run with the same version changes nothing and says so: it
// downloads SHA256SUMS only, and the binary keeps its inode and its time.
#[test]
fn a_second_run_changes_nothing() {
    let lab = Lab::new("second");
    lab.curl_with(SERVE);
    let out = lab.run(&[]);
    assert!(out.status.success(), "{}", text(&out));
    let before = stamp(&lab.dest().join("podnet"));
    std::fs::remove_file(lab.root.join("curl.log")).unwrap();
    let out = lab.run(&[]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("podnet 9.9.9 is already installed"), "{t}");
    assert_eq!(stamp(&lab.dest().join("podnet")), before, "{t}");
    let log = lab.curl_log();
    assert_eq!(log.lines().count(), 1, "{log}");
    assert!(log.ends_with("/SHA256SUMS\n"), "{log}");
    assert_eq!(lab.entries(&lab.dest()), vec!["podnet"]);
}

// A downloader that the core does not know (aria2c) still installs: the
// core takes the download of podnet.
#[test]
fn aria2c_installs_through_the_core() {
    let lab = Lab::new("aria2c");
    lab.script(
        "aria2c",
        &format!(
            "d=''; o=''; url=''\nwhile [ $# -gt 0 ]; do case \"$1\" in -d) d=$2; shift 2 ;; -o) o=$2; shift 2 ;; -*) shift ;; *) url=$1; shift ;; esac; done\nf={rel}/$(basename \"$url\")\n[ -f \"$f\" ] || exit 3\ncp \"$f\" \"$d/$o\"",
            rel = lab.root.join("release").display()
        ),
    );
    let out = lab.run(&[]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("downloader: aria2c"), "{t}");
    assert!(t.contains("podnet 9.9.9 (test)"), "{t}");
}

// With no SHA-256 tool, the run fails; PODNET_NO_VERIFY=1 installs with a
// warning.
#[test]
fn no_sha256_tool_needs_no_verify() {
    let lab = Lab::new("nosha");
    lab.curl();
    std::fs::remove_file(lab.bin().join("sha256sum")).unwrap();
    let out = lab.run(&[]);
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("set PODNET_NO_VERIFY=1"), "{t}");
    assert!(!lab.dest().join("podnet").exists());
    let out = lab.run(&[("PODNET_NO_VERIFY", "1")]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("is not checked (PODNET_NO_VERIFY=1)"), "{t}");
    assert!(lab.dest().join("podnet").exists());
}

/// The shells of scripts/install-test.sh that this host has.
fn shells() -> Vec<&'static str> {
    [
        ("dash", "dash"),
        ("bash", "bash --posix"),
        ("busybox", "busybox sh"),
    ]
    .into_iter()
    .filter(|(tool, _)| which(tool).is_some())
    .map(|(_, shell)| shell)
    .collect()
}

fn install_test(root: &Path, shell: &str) -> Output {
    Command::new("sh")
        .arg(root.join("scripts/install-test.sh"))
        .arg(shell)
        .output()
        .unwrap()
}

// scripts/install-test.sh passes in each POSIX shell of this host: a fake
// release on the loopback and in a file:// directory, a second run, a dry
// run, a wrong SHA-256, the copy of a release with its pins, offline.
#[test]
fn the_install_test_passes_in_each_shell() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    // The fake release is served on 127.0.0.1.
    crate::test_host::need_loopback!();
    if which("python3").is_none() {
        eprintln!("skip: no python3 for the fake release");
        return;
    }
    let shells = shells();
    assert!(!shells.is_empty(), "no POSIX shell of the list");
    for shell in shells {
        let out = install_test(root, shell);
        assert!(out.status.success(), "{shell}: {}", text(&out));
    }
}

// A planted defect: a copy of the installer that does not compare the
// SHA-256 with the pin fails scripts/install-test.sh.
#[test]
fn an_installer_with_no_pin_check_fails_the_install_test() {
    crate::test_host::need_loopback!();
    if which("python3").is_none() || which("dash").is_none() {
        eprintln!("skip: no python3 or no dash");
        return;
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tree = std::env::temp_dir().join(format!("podnet-install-plant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tree);
    std::fs::create_dir_all(tree.join("scripts/install")).unwrap();
    for f in ["install-test.sh", "install/core.sh", "install/pins.py"] {
        std::fs::copy(src.join("scripts").join(f), tree.join("scripts").join(f)).unwrap();
    }
    let script = std::fs::read_to_string(src.join("scripts/install.sh")).unwrap();
    let check = "if [ \"$inst_v\" != \"$(inst_pin \"$1\")\" ]; then";
    assert_eq!(script.matches(check).count(), 1);
    std::fs::write(
        tree.join("scripts/install.sh"),
        script.replace(check, "if false; then"),
    )
    .unwrap();
    let out = install_test(&tree, "dash");
    let _ = std::fs::remove_dir_all(&tree);
    let t = text(&out);
    assert!(!out.status.success(), "{t}");
    assert!(t.contains("FAIL dash: wrong SHA-256: exit 0"), "{t}");
}
