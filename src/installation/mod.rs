//! Integração por usuário. Não acessa USB, rede ou perfis de outros programas.
use crate::storage::{self, paths::AppPaths};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Seek, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

const MARKER: &str = "meta/installation.json";
#[derive(Serialize, Deserialize)]
struct Manifest {
    application: String,
    schema: u8,
}

fn owned(root: &Path) -> Result<()> {
    ensure!(
        !fs::symlink_metadata(root)?.file_type().is_symlink(),
        "A pasta PeliCool não pode ser um link simbólico."
    );
    let marker = root.join(MARKER);
    ensure!(
        fs::metadata(&marker)?.len() < 4096,
        "Registro de instalação inválido."
    );
    let manifest: Manifest = serde_json::from_slice(&fs::read(marker)?)?;
    ensure!(
        manifest.application == "PeliCool" && manifest.schema == 1,
        "Esta pasta não pertence ao instalador do PeliCool."
    );
    Ok(())
}

fn desktop(paths: &AppPaths) -> Result<String> {
    let executable = paths.appimage();
    let path = executable
        .to_str()
        .context("Caminho de instalação não é UTF-8")?;
    ensure!(
        !path.contains(['\n', '\r']),
        "Caminho de instalação inválido."
    );
    let quoted = path
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%");
    let icon = paths.installation.join("share/icons/pelicool.svg");
    let icon = icon
        .to_str()
        .context("Caminho do ícone inválido")?
        .replace('\\', "\\\\");
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=PeliCool\nComment=Personalização de displays LCD e iluminação\nExec=\"{quoted}\"\nIcon={icon}\nTerminal=false\nCategories=Utility;\nStartupWMClass=pelicool\nX-PeliCool-Managed=true\n"
    ))
}

pub fn is_installed(paths: &AppPaths) -> bool {
    owned(&paths.installation).is_ok() && paths.appimage().is_file()
}

pub fn install(paths: &AppPaths, source: &Path) -> Result<()> {
    // Verifica o formato antes de criar ou substituir qualquer arquivo.
    let mut file = fs::File::open(source).context("Selecione o AppImage original do PeliCool")?;
    let mut header = [0u8; 11];
    file.read_exact(&mut header)?;
    ensure!(
        &header[..4] == b"\x7fELF" && &header[8..11] == b"AI\x02",
        "O arquivo precisa ser um AppImage tipo 2."
    );
    let contents = desktop(paths)?;
    let _lock = storage::configuration_lock(&paths.config, true)?;
    if paths.desktop().exists() {
        ensure!(
            fs::read_to_string(paths.desktop())? == contents,
            "Já existe um lançador diferente em {}. Revise-o antes de instalar.",
            paths.desktop().display()
        );
    }
    if paths.installation.exists() {
        owned(&paths.installation)?;
    } else {
        storage::private_dir(&paths.installation)?;
        storage::write_json(
            &paths.installation.join(MARKER),
            &Manifest {
                application: "PeliCool".into(),
                schema: 1,
            },
        )?;
    }
    for folder in ["bin", "share", "share/icons", "share/licenses", "meta"] {
        let path = paths.installation.join(folder);
        ensure!(
            !path.is_symlink(),
            "Diretório de instalação redirecionado: {}",
            path.display()
        );
        storage::private_dir(&path)?;
    }
    fs::write(
        paths.installation.join("share/icons/pelicool.svg"),
        include_bytes!("../../assets/pelicool-icon.svg"),
    )?;
    fs::write(
        paths.installation.join("share/licenses/LICENSE"),
        include_bytes!("../../LICENSE"),
    )?;
    fs::write(
        paths.installation.join("share/licenses/gpteixeira-MIT.txt"),
        include_bytes!("../../LICENSES/gpteixeira-MIT.txt"),
    )?;
    fs::write(
        paths
            .installation
            .join("share/licenses/THIRD-PARTY-NOTICES.md"),
        include_bytes!("../../THIRD-PARTY-NOTICES.md"),
    )?;
    let temporary = paths
        .installation
        .join(format!("bin/.install-{}.tmp", std::process::id()));
    let result = (|| -> Result<()> {
        let mut destination = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o700)
            .open(&temporary)?;
        file.rewind()?;
        std::io::copy(&mut file, &mut destination)?;
        destination.flush()?;
        destination.sync_all()?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
        fs::rename(&temporary, paths.appimage())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result?;
    storage::private_dir(&paths.applications)?;
    fs::write(paths.desktop(), contents)?;
    Ok(())
}

pub fn uninstall(paths: &AppPaths, purge: bool) -> Result<()> {
    let _lock = storage::configuration_lock(&paths.config, true)?;
    if paths.installation.exists() {
        owned(&paths.installation)?;
    }
    set_autostart(paths, false)?;
    if paths.desktop().exists() {
        ensure!(
            fs::read_to_string(paths.desktop())? == desktop(paths)?,
            "O lançador foi modificado; revise {} antes de removê-lo.",
            paths.desktop().display()
        );
        fs::remove_file(paths.desktop())?;
    }
    if paths.installation.exists() {
        fs::remove_dir_all(&paths.installation)?;
    }
    if purge {
        // Somente o diretório do aplicativo. Originais das mídias e OpenRGB ficam intactos.
        ensure!(
            !paths.config.is_symlink(),
            "Configuração redirecionada; limpeza interrompida."
        );
        purge_legacy(paths)?;
        fs::remove_dir_all(&paths.config)?;
    }
    Ok(())
}

fn purge_legacy(paths: &AppPaths) -> Result<()> {
    let old = &paths.legacy;
    if !old.exists() {
        return Ok(());
    }
    ensure!(
        !old.is_symlink(),
        "Backup antigo é um link; revise-o manualmente."
    );
    let catalog = old.join("collection.json");
    ensure!(
        catalog.is_file() && fs::metadata(&catalog)?.len() <= 1024 * 1024,
        "Backup antigo desconhecido; limpeza preservou {}.",
        old.display()
    );
    let parsed: crate::collection::Catalog = serde_json::from_slice(&fs::read(catalog)?)?;
    parsed.validate()?;
    for entry in fs::read_dir(old)? {
        let name = entry?.file_name();
        ensure!(
            [
                "collection.json",
                "collection.json.tmp",
                "collection.lock",
                "media",
                "devices"
            ]
            .iter()
            .any(|allowed| name == *allowed),
            "Backup contém arquivos não reconhecidos; revise {}.",
            old.display()
        );
    }
    // Um cliente antigo ainda aberto impede a remoção do backup.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(old.join("collection.lock"))?;
    lock.try_lock()
        .context("Feche a versão anterior antes de limpar seu backup.")?;
    fs::remove_dir_all(old)?;
    Ok(())
}

fn autostart_path(paths: &AppPaths) -> std::path::PathBuf {
    paths
        .config
        .parent()
        .expect("XDG root")
        .join("autostart/pelicool.desktop")
}
fn autostart_entry(paths: &AppPaths) -> Result<String> {
    let entry = desktop(paths)?;
    Ok(entry
        .lines()
        .map(|line| {
            if line.starts_with("Exec=") {
                format!("{line} gui --start-minimized")
            } else {
                line.into()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n")
}
pub fn autostart_enabled(paths: &AppPaths) -> bool {
    autostart_entry(paths).ok().is_some_and(|expected| {
        fs::read_to_string(autostart_path(paths)).is_ok_and(|entry| entry == expected)
    })
}
pub fn set_autostart(paths: &AppPaths, enabled: bool) -> Result<()> {
    let path = autostart_path(paths);
    let entry = autostart_entry(paths)?;
    if path.exists() {
        ensure!(
            fs::read_to_string(&path)? == entry,
            "Já existe uma inicialização automática diferente; revise {}.",
            path.display()
        );
    }
    if enabled {
        ensure!(
            is_installed(paths),
            "Instale o AppImage antes de ativar a inicialização automática."
        );
        storage::private_dir(path.parent().expect("autostart parent"))?;
        fs::write(path, entry)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}
