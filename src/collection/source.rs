//! Localização de originais antigos. Não varre o disco nem depende de um projeto fixo.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub(super) fn resolve(source: &Path) -> Result<PathBuf> {
    let mut roots = vec![];
    if let Ok(cwd) = std::env::current_dir() {
        roots.extend(cwd.ancestors().take(4).map(Path::to_path_buf));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.extend(parent.ancestors().take(4).map(Path::to_path_buf));
    }
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        let path = PathBuf::from(appimage);
        if path.is_absolute()
            && let Some(parent) = path.parent()
        {
            roots.extend(parent.ancestors().take(3).map(Path::to_path_buf));
        }
    }
    resolve_at(source, &roots)
}

fn resolve_at(source: &Path, roots: &[PathBuf]) -> Result<PathBuf> {
    if source.is_file() {
        return source
            .canonicalize()
            .context("Não foi possível localizar o original");
    }
    if !source.is_absolute() {
        for root in roots.iter().filter(|p| p.parent().is_some()) {
            let candidate = root.join(source);
            if candidate.is_file() {
                return Ok(candidate.canonicalize()?);
            }
        }
    }
    anyhow::bail!(
        "Original não encontrado: {}. Localize o arquivo novamente para editar o enquadramento.",
        source.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_relative_source_is_resolved_from_launch_roots_without_changing_cwd() {
        let root = std::env::temp_dir().join(format!("pelicool-source-{}", std::process::id()));
        std::fs::create_dir_all(root.join("media")).unwrap();
        std::fs::write(root.join("media/old.gif"), b"GIF89a").unwrap();
        let path = resolve_at(Path::new("media/old.gif"), std::slice::from_ref(&root)).unwrap();
        assert_eq!(path, root.join("media/old.gif").canonicalize().unwrap());
        assert!(resolve_at(Path::new("missing.gif"), std::slice::from_ref(&root)).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
