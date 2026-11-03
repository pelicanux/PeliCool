mod common;
use common::TempDir;
use pelicool::{
    installation,
    storage::{self, preferences::Interface},
};
use std::fs;
fn image(temp: &TempDir) -> std::path::PathBuf {
    let source = temp.0.join("PeliCool.AppImage");
    let mut bytes = vec![0u8; 64];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[8..11].copy_from_slice(b"AI\x02");
    fs::write(&source, bytes).unwrap();
    source
}
#[test]
fn install_autostart_uninstall_and_purge_preserve_originals_and_foreign_configuration() {
    let temp = TempDir::new("install");
    let paths = temp.paths();
    let source = image(&temp);
    let unrelated = temp.0.join("config/OpenRGB/profile.json");
    fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
    fs::write(&unrelated, "keep").unwrap();
    Interface {
        dark: true,
        ..Default::default()
    }
    .save(&paths.config)
    .unwrap();
    installation::install(&paths, &source).unwrap();
    assert!(installation::is_installed(&paths));
    assert_eq!(
        fs::read_to_string(paths.installation.join("share/licenses/gpteixeira-MIT.txt")).unwrap(),
        include_str!("../LICENSES/gpteixeira-MIT.txt")
    );
    assert!(
        !paths
            .installation
            .join("share/licenses/Jersey10-OFL.txt")
            .exists()
    );
    assert_eq!(
        fs::read(paths.appimage()).unwrap(),
        fs::read(&source).unwrap()
    );
    installation::set_autostart(&paths, true).unwrap();
    assert!(installation::autostart_enabled(&paths));
    let lock = storage::configuration_lock(&paths.config, false).unwrap();
    assert!(installation::uninstall(&paths, true).is_err());
    assert!(paths.appimage().exists());
    drop(lock);
    installation::uninstall(&paths, false).unwrap();
    assert!(!paths.installation.exists());
    assert!(!paths.desktop().exists());
    assert!(!installation::autostart_enabled(&paths));
    assert!(Interface::load(&paths.config).unwrap().dark);
    installation::uninstall(&paths, true).unwrap();
    assert!(!paths.config.exists());
    assert!(source.exists());
    assert_eq!(fs::read_to_string(unrelated).unwrap(), "keep");
}
#[test]
fn installer_refuses_foreign_directories_and_non_appimages() {
    let temp = TempDir::new("foreign");
    let paths = temp.paths();
    fs::create_dir_all(&paths.installation).unwrap();
    fs::write(paths.installation.join("personal.txt"), "keep").unwrap();
    assert!(installation::install(&paths, &image(&temp)).is_err());
    assert!(installation::uninstall(&paths, true).is_err());
    assert!(paths.installation.join("personal.txt").exists());
    let temp = TempDir::new("invalid-appimage");
    let paths = temp.paths();
    let source = temp.0.join("wrong");
    fs::write(&source, b"not executable").unwrap();
    assert!(installation::install(&paths, &source).is_err());
    assert!(!paths.installation.exists());
}
#[test]
fn modified_launchers_are_preserved_and_paths_with_spaces_are_quoted() {
    let temp = TempDir::new("desktop");
    let paths = pelicool::storage::paths::AppPaths::from_roots(
        &temp.0,
        Some(temp.0.join("config")),
        Some(temp.0.join("data space $ quote\" %")),
    )
    .unwrap();
    installation::install(&paths, &image(&temp)).unwrap();
    let desktop = fs::read_to_string(paths.desktop()).unwrap();
    assert!(desktop.contains("Exec=\"/"));
    assert!(desktop.contains("\\$"));
    assert!(desktop.contains("%%"));
    fs::write(paths.desktop(), "foreign desktop").unwrap();
    assert!(installation::uninstall(&paths, false).is_err());
    assert!(paths.installation.exists());
    assert_eq!(
        fs::read_to_string(paths.desktop()).unwrap(),
        "foreign desktop"
    );
}
