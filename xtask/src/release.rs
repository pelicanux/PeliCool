//! Build de distribuição sem caminhos da conta do desenvolvedor nos binários.
use anyhow::{Context, Result, ensure};
use std::{env, path::Path, process::Command};

pub fn build(root: &Path) -> Result<()> {
    let mut flags: Vec<String> = if let Some(flags) = env::var_os("CARGO_ENCODED_RUSTFLAGS") {
        flags
            .to_str()
            .context("Flags Rust inválidas")?
            .split('\x1f')
            .filter(|flag| !flag.is_empty())
            .map(str::to_owned)
            .collect()
    } else {
        env::var("RUSTFLAGS")
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    };
    if let Some(home) = env::var_os("HOME") {
        let home = Path::new(&home);
        let cargo = env::var_os("CARGO_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".cargo"));
        let rustup = env::var_os("RUSTUP_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| home.join(".rustup"));
        // rustc dá prioridade ao último prefixo coincidente: os específicos
        // ficam depois da raiz da conta. O separador evita quebrar caminhos com espaços.
        for (path, replacement) in [
            (home, "/build/user"),
            (cargo.as_path(), "/build/cargo"),
            (rustup.as_path(), "/build/rustup"),
        ] {
            if path.is_absolute() {
                flags.push(format!(
                    "--remap-path-prefix={}={replacement}",
                    path.display()
                ));
            }
        }
    }
    flags.push(format!(
        "--remap-path-prefix={}=/build/pelicool",
        root.display()
    ));
    let status = Command::new("cargo")
        .args(["build", "--locked", "--release", "-p", "pelicool"])
        .env("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"))
        .current_dir(root)
        .status()
        .context("Não foi possível iniciar o build de distribuição")?;
    ensure!(status.success(), "O build de distribuição falhou");
    Ok(())
}
