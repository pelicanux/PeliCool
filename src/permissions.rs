//! Regras udev por IDs exatos; autenticação administrativa isolada da GUI.
use anyhow::{Context, Result, bail, ensure};
use std::{fs, io::Write, os::unix::fs::OpenOptionsExt, path::Path, process::Command};

pub const RULE: &str = include_str!("../udev/70-pelicool.rules");
pub const RULE_PATH: &str = "/etc/udev/rules.d/70-pelicool.rules";
const LEGACY: &str = "70-deepcool-linux.rules";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleState {
    Missing,
    Installed,
    Legacy,
    Modified,
    Unreadable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Remove,
}
impl Action {
    pub fn argument(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Remove => "remove",
        }
    }
}

fn inspect(path: &Path) -> Result<Option<bool>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() || metadata.len() > 65536 {
        return Ok(Some(false));
    }
    Ok(Some(fs::read(path)? == RULE.as_bytes()))
}

pub fn rule_state(directory: &Path) -> RuleState {
    let current = inspect(&directory.join("70-pelicool.rules"));
    let legacy = inspect(&directory.join(LEGACY));
    match (current, legacy) {
        (Err(_), _) | (_, Err(_)) => RuleState::Unreadable,
        (Ok(Some(false)), _) | (_, Ok(Some(false))) => RuleState::Modified,
        (Ok(Some(true)), _) => RuleState::Installed,
        (_, Ok(Some(true))) => RuleState::Legacy,
        _ => RuleState::Missing,
    }
}

/// Só abre o nó para verificar permissões do usuário; não lê nem envia pacotes.
pub fn usb_access(info: &crate::drivers::DriverInfo) -> Option<bool> {
    let entries = fs::read_dir("/sys/bus/usb/devices").ok()?;
    let mut accessible = true;
    let mut found = false;
    for entry in entries.flatten() {
        let root = entry.path();
        let hex = |name| {
            fs::read_to_string(root.join(name))
                .ok()
                .and_then(|value| u16::from_str_radix(value.trim(), 16).ok())
        };
        let Some(vendor) = hex("idVendor") else {
            continue;
        };
        let Some(product) = hex("idProduct") else {
            continue;
        };
        if !info
            .usb
            .ids
            .iter()
            .any(|id| id.vendor == vendor && id.product == product)
        {
            continue;
        }
        let number = |name| {
            fs::read_to_string(root.join(name))
                .ok()
                .and_then(|value| value.trim().parse::<u16>().ok())
        };
        let (Some(bus), Some(device)) = (number("busnum"), number("devnum")) else {
            continue;
        };
        found = true;
        accessible &= fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(format!("/dev/bus/usb/{bus:03}/{device:03}"))
            .is_ok();
    }
    found.then_some(accessible)
}

/// A GUI continua sem root. pkexec autentica somente o subcomando administrativo.
pub fn authenticated(action: Action) -> Result<String> {
    let executable = std::env::current_exe().context("Não foi possível localizar o PeliCool")?;
    let result = Command::new("pkexec").arg(executable)
        .args(["permissions", action.argument()]).output()
        .context("Não foi possível abrir a autenticação do sistema. Instale polkit/pkexec ou use o comando sudo indicado nas configurações.")?;
    authentication_result(result)
}

fn authentication_result(result: std::process::Output) -> Result<String> {
    match result.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned()),
        Some(126) => bail!(
            "Instalação ou remoção cancelada na autenticação. Você pode tentar novamente nas configurações."
        ),
        Some(127) => bail!(
            "Autenticação administrativa não autorizada ou indisponível. Tente novamente nas configurações."
        ),
        _ => bail!(
            "Operação udev não concluída: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ),
    }
}

/// Escrita restrita aos arquivos gerenciados. Valida todos antes de alterar algum.
fn update_rule(directory: &Path, action: Action) -> Result<()> {
    ensure!(
        matches!(
            rule_state(directory),
            RuleState::Missing | RuleState::Installed | RuleState::Legacy
        ),
        "Existe uma regra modificada, redirecionada ou não legível. Ela foi preservada; revise os arquivos udev manualmente."
    );
    let current = directory.join("70-pelicool.rules");
    let legacy = directory.join(LEGACY);
    if action == Action::Install {
        fs::create_dir_all(directory)?;
        let temporary = directory.join(format!(".pelicool-{}.tmp", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o644)
            .open(&temporary)?;
        let result = (|| -> Result<()> {
            file.write_all(RULE.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &current)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
    } else if current.exists() {
        fs::remove_file(current)?;
    }
    if legacy.exists() {
        fs::remove_file(legacy)?;
    }
    Ok(())
}

fn udevadm(arguments: &[&str]) -> Result<()> {
    let result = Command::new("udevadm")
        .args(arguments)
        .output()
        .context("udevadm não encontrado")?;
    ensure!(
        result.status.success(),
        "Falha ao atualizar udev: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    Ok(())
}

pub fn apply(action: Action) -> Result<String> {
    let status = fs::read_to_string("/proc/self/status")?;
    let effective = status
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(2));
    ensure!(
        effective == Some("0"),
        "Esta operação requer autenticação administrativa. Use o botão do PeliCool ou sudo pelicool permissions {}.",
        action.argument()
    );
    update_rule(Path::new("/etc/udev/rules.d"), action)?;
    udevadm(&["control", "--reload-rules"])?;
    // Um erro aqui não desfaz o arquivo instalado/removido: informe esse estado.
    let refresh = (|| -> Result<()> {
        for module in crate::drivers::REGISTERED {
            for id in module.info.usb.ids {
                udevadm(&[
                    "trigger",
                    "--action=add",
                    "--subsystem-match=usb",
                    &format!("--attr-match=idVendor={:04x}", id.vendor),
                    &format!("--attr-match=idProduct={:04x}", id.product),
                ])?;
            }
        }
        udevadm(&["settle", "--timeout=5"])
    })();
    let message = match action {
        Action::Install => "Regra PeliCool instalada. Use o aplicativo sem root.",
        Action::Remove => "Regra PeliCool removida. Reconecte o USB para renovar as permissões.",
    };
    Ok(match refresh {
        Ok(()) => message.into(),
        Err(_) => format!(
            "{message} Reconecte o USB: não foi possível renovar as permissões automaticamente."
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authentication_cancel_and_failure_do_not_report_success() {
        use std::os::unix::process::ExitStatusExt;
        for (code, expected) in [
            (126, "cancelada"),
            (127, "indisponível"),
            (1, "erro de teste"),
        ] {
            let output = std::process::Output {
                status: std::process::ExitStatus::from_raw(code << 8),
                stdout: vec![],
                stderr: b"erro de teste".to_vec(),
            };
            assert!(
                authentication_result(output)
                    .unwrap_err()
                    .to_string()
                    .contains(expected)
            );
        }
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"Regra instalada\n".to_vec(),
            stderr: vec![],
        };
        assert_eq!(authentication_result(output).unwrap(), "Regra instalada");
    }
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root =
                std::env::temp_dir().join(format!("pelicool-udev-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn install_remove_and_legacy_upgrade_preserve_unrelated_rules() {
        let fixture = Fixture::new();
        let root = &fixture.0;
        fs::write(root.join("other.rules"), "foreign").unwrap();
        assert_eq!(rule_state(root), RuleState::Missing);
        fs::write(root.join(LEGACY), RULE).unwrap();
        assert_eq!(rule_state(root), RuleState::Legacy);
        update_rule(root, Action::Install).unwrap();
        assert_eq!(rule_state(root), RuleState::Installed);
        assert!(!root.join(LEGACY).exists());
        update_rule(root, Action::Install).unwrap();
        update_rule(root, Action::Remove).unwrap();
        assert_eq!(rule_state(root), RuleState::Missing);
        assert_eq!(
            fs::read_to_string(root.join("other.rules")).unwrap(),
            "foreign"
        );
    }
    #[test]
    fn modified_or_symlinked_rules_are_never_overwritten_or_removed() {
        let fixture = Fixture::new();
        let root = &fixture.0;
        fs::write(root.join("70-pelicool.rules"), RULE).unwrap();
        fs::write(root.join(LEGACY), "custom").unwrap();
        for action in [Action::Install, Action::Remove] {
            assert!(update_rule(root, action).is_err());
        }
        assert_eq!(
            fs::read_to_string(root.join("70-pelicool.rules")).unwrap(),
            RULE
        );
        fs::remove_file(root.join(LEGACY)).unwrap();
        fs::remove_file(root.join("70-pelicool.rules")).unwrap();
        fs::write(root.join("foreign"), RULE).unwrap();
        std::os::unix::fs::symlink(root.join("foreign"), root.join("70-pelicool.rules")).unwrap();
        assert_eq!(rule_state(root), RuleState::Modified);
        assert!(update_rule(root, Action::Remove).is_err());
        assert_eq!(fs::read_to_string(root.join("foreign")).unwrap(), RULE);
    }
}
