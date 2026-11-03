//! Executa o mesmo coletor independente distribuído em scripts/. Sem rede ou USB.
use anyhow::{Context, Result, ensure};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

const SCRIPT: &str = include_str!("../scripts/pelicool-diagnose.sh");

pub fn command() -> Command {
    let mut command = Command::new("sh");
    command.args(["-c", SCRIPT, "pelicool-diagnose"]);
    command
}

pub fn collect(output: &Path) -> Result<PathBuf> {
    let result = command()
        .arg("--consent")
        .arg("--output")
        .arg(output)
        .output()
        .context("Não foi possível executar o coletor local")?;
    ensure!(
        result.status.success(),
        "Coleta interrompida: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    Ok(output.to_path_buf())
}
