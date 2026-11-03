mod common;
use common::TempDir;
use std::{fs, process::Command};
fn command() -> Command {
    let mut command = Command::new("sh");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/pelicool-diagnose.sh"
    ));
    command
}
#[test]
fn collector_requires_consent_and_omits_serials_unrelated_usb_and_personal_files() {
    let temp = TempDir::new("diagnostics");
    let usb = temp.0.join("usb");
    let hw = temp.0.join("hwmon");
    fs::create_dir_all(&hw).unwrap();
    for (name, vendor, maker) in [
        ("1-1", "3633", "DeepCool"),
        ("1-2", "1234", "Keyboard"),
        ("1-3", "abcd", "DeepCool Labs"),
    ] {
        let path = usb.join(name);
        fs::create_dir_all(&path).unwrap();
        for (attr, value) in [
            ("idVendor", vendor),
            ("idProduct", "0009"),
            ("manufacturer", maker),
            ("serial", "PRIVATE-SERIAL"),
            ("product", "Test display"),
        ] {
            fs::write(path.join(attr), value).unwrap();
        }
    }
    let output = temp.0.join("report");
    let no = command()
        .args(["--allow-root", "--output"])
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(!no.status.success());
    assert!(!output.exists());
    let yes = command()
        .args(["--allow-root", "--consent", "--sysfs-root"])
        .arg(&usb)
        .arg("--hwmon-root")
        .arg(&hw)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        yes.status.success(),
        "{}",
        String::from_utf8_lossy(&yes.stderr)
    );
    let mut contents = String::new();
    for entry in fs::read_dir(output.join("usb")).unwrap() {
        contents += &fs::read_to_string(entry.unwrap().path()).unwrap();
    }
    assert!(contents.contains("DeepCool"));
    assert!(!contents.contains("PRIVATE-SERIAL"));
    assert!(!contents.contains("Keyboard"));
    assert_eq!(fs::read_dir(output.join("usb")).unwrap().count(), 2);
    let again = command()
        .args(["--allow-root", "--consent", "--output"])
        .arg(&output)
        .output()
        .unwrap();
    assert!(!again.status.success());
}
