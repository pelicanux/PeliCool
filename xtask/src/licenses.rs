//! Reúne avisos do grafo de dependências do aplicativo para o AppImage.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const LIMIT: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct Metadata {
    packages: Vec<Package>,
    resolve: Resolve,
}

#[derive(Deserialize)]
struct Package {
    id: String,
    name: String,
    version: String,
    manifest_path: PathBuf,
    license: Option<String>,
    license_file: Option<PathBuf>,
    #[serde(default)]
    authors: Vec<String>,
}

#[derive(Deserialize)]
struct Resolve {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Node {
    id: String,
    deps: Vec<Dependency>,
}

#[derive(Deserialize)]
struct Dependency {
    pkg: String,
    dep_kinds: Vec<Kind>,
}

#[derive(Deserialize)]
struct Kind {
    kind: Option<String>,
}

impl Metadata {
    fn application_packages(&self) -> Result<Vec<&Package>> {
        let root = self
            .packages
            .iter()
            .find(|package| package.name == "pelicool")
            .context("Pacote pelicool ausente nos metadados")?;
        let nodes: BTreeMap<_, _> = self
            .resolve
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        let mut pending = vec![root.id.as_str()];
        let mut visited = BTreeSet::new();
        while let Some(id) = pending.pop() {
            if !visited.insert(id) {
                continue;
            }
            let node = nodes.get(id).context("Grafo de dependências incompleto")?;
            pending.extend(
                node.deps
                    .iter()
                    .filter(|dependency| {
                        dependency
                            .dep_kinds
                            .iter()
                            .any(|kind| kind.kind.as_deref() != Some("dev"))
                    })
                    .map(|dependency| dependency.pkg.as_str()),
            );
        }
        let mut packages: Vec<_> = self
            .packages
            .iter()
            .filter(|package| package.id != root.id && visited.contains(package.id.as_str()))
            .collect();
        packages.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
        Ok(packages)
    }
}

fn copy_notice(source: &Path, package_root: &Path, destination: &Path) -> Result<bool> {
    if !source.is_file() {
        return Ok(false);
    }
    ensure!(
        source.canonicalize()?.starts_with(package_root),
        "Aviso de licença fora do pacote"
    );
    if source.metadata()?.len() > LIMIT {
        return Ok(false);
    }
    fs::copy(
        source,
        destination.join(source.file_name().context("Aviso sem nome de arquivo")?),
    )?;
    Ok(true)
}

fn write_notices(packages: &[&Package], destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut index = String::from(
        "# Dependências Rust\n\nAvisos dos pacotes originais; licenças próprias não são substituídas pela MIT do PeliCool.\n",
    );
    for package in packages {
        let name = format!("{}-{}", package.name, package.version);
        ensure!(
            name.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.+".contains(&byte)),
            "Identificador de pacote inválido"
        );
        let folder = package
            .manifest_path
            .parent()
            .context("Manifesto sem pasta")?
            .canonicalize()?;
        let output = destination.join("rust").join(&name);
        fs::create_dir_all(&output)?;
        let mut copied = 0;
        for entry in fs::read_dir(&folder)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_uppercase();
            if [
                "LICENSE",
                "LICENCE",
                "COPYING",
                "NOTICE",
                "COPYRIGHT",
                "AUTHORS",
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            {
                copied += usize::from(copy_notice(&entry.path(), &folder, &output)?);
            }
        }
        if let Some(license) = &package.license_file {
            copied += usize::from(copy_notice(&folder.join(license), &folder, &output)?);
        }
        index.push_str(&format!(
            "\n- {name}: {}; autores: {}",
            package
                .license
                .as_deref()
                .unwrap_or("ver arquivo de licença"),
            package.authors.join(", ")
        ));
        if copied == 0 {
            index.push_str(&format!(
                "\n  Fonte para revisão dos avisos: https://crates.io/crates/{}/{}",
                package.name, package.version
            ));
        }
    }
    index.push('\n');
    fs::write(destination.join("rust-dependencies.md"), index)?;
    Ok(())
}

pub fn collect(root: &Path, destination: &Path) -> Result<()> {
    let result = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "x86_64-unknown-linux-gnu",
        ])
        .current_dir(root)
        .output()
        .context("Não foi possível consultar cargo metadata")?;
    ensure!(
        result.status.success(),
        "cargo metadata falhou: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: Metadata = serde_json::from_slice(&result.stdout)?;
    let packages = metadata.application_packages()?;
    write_notices(&packages, destination)?;
    // libudev é a biblioteca do sistema-base copiada pelo linuxdeploy.
    for folder in [
        Path::new("/usr/share/licenses/systemd"),
        Path::new("/usr/share/doc/libudev1"),
    ] {
        if !folder.is_dir() {
            continue;
        }
        let output = destination.join("libudev");
        fs::create_dir_all(&output)?;
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            if path.is_file() && path.metadata()?.len() <= LIMIT {
                fs::copy(
                    &path,
                    output.join(path.file_name().context("Aviso sem nome")?),
                )?;
            }
        }
        break;
    }
    println!(
        "Licenças de {} dependências reunidas em {}.",
        packages.len(),
        destination.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn application_graph_excludes_dev_tools_and_keeps_transitive_build_dependencies() {
        let package = |name| json!({"id":name, "name":name, "version":"1.0.0", "manifest_path":"/unused/Cargo.toml"});
        let dep = |name, kind| json!({"pkg":name, "dep_kinds":[{"kind":kind}]});
        let metadata: Metadata = serde_json::from_value(json!({
            "packages":[package("pelicool"),package("runtime"),package("build"),package("dev"),package("pelicool-xtask")],
            "resolve":{"nodes":[
                {"id":"pelicool","deps":[dep("runtime",None::<&str>),dep("dev",Some("dev"))]},
                {"id":"runtime","deps":[dep("build",Some("build"))]},
                {"id":"build","deps":[]}, {"id":"dev","deps":[]},
                {"id":"pelicool-xtask","deps":[]}
            ]}
        })).unwrap();
        let names: Vec<_> = metadata
            .application_packages()
            .unwrap()
            .into_iter()
            .map(|package| package.name.as_str())
            .collect();
        assert_eq!(names, ["build", "runtime"]);
    }

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("pelicool-licenses-{}-{nonce}", std::process::id()));
            fs::create_dir_all(path.join("package/legal")).unwrap();
            Self(path)
        }
        fn package(&self) -> Package {
            serde_json::from_value(json!({
                "id":"example", "name":"example", "version":"1.2.3",
                "manifest_path":self.0.join("package/Cargo.toml"),
                "license":"MIT", "license_file":"legal/terms.txt", "authors":["Example Author"]
            }))
            .unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn notices_keep_declared_terms_and_authors_without_source_paths_or_unrelated_files() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("package/LICENSE-MIT"), "MIT terms").unwrap();
        fs::write(fixture.0.join("package/legal/terms.txt"), "Declared terms").unwrap();
        fs::write(fixture.0.join("package/source.rs"), "private source").unwrap();
        let output = fixture.0.join("output");
        write_notices(&[&fixture.package()], &output).unwrap();
        let copied = output.join("rust/example-1.2.3");
        assert_eq!(
            fs::read_to_string(copied.join("LICENSE-MIT")).unwrap(),
            "MIT terms"
        );
        assert_eq!(
            fs::read_to_string(copied.join("terms.txt")).unwrap(),
            "Declared terms"
        );
        assert!(!copied.join("source.rs").exists());
        let index = fs::read_to_string(output.join("rust-dependencies.md")).unwrap();
        assert!(index.contains("example-1.2.3: MIT; autores: Example Author"));
        assert!(!index.contains(fixture.0.to_str().unwrap()));
        fs::write(fixture.0.join("outside.txt"), "unrelated").unwrap();
        let mut package = fixture.package();
        package.license_file = Some("../outside.txt".into());
        assert!(write_notices(&[&package], &output).is_err());
        assert!(!copied.join("outside.txt").exists());
    }
}
