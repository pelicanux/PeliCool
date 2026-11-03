//! Ferramentas locais em Rust, separadas do aplicativo e ausentes no AppImage.
mod brand;
mod licenses;
mod release;

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be a workspace member")
}

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    match args.next().as_deref().and_then(|value| value.to_str()) {
        Some("brand") if args.len() == 0 => brand::export(root()),
        Some("release") if args.len() == 0 => release::build(root()),
        Some("licenses") if args.len() == 1 => {
            let destination = PathBuf::from(args.next().context("Informe a pasta de licenças")?);
            licenses::collect(root(), &destination)
        }
        Some("--help" | "-h") | None => {
            println!(
                "Ferramentas do PeliCool\n\
                 cargo run --locked -p pelicool-xtask -- brand\n\
                 cargo run --locked -p pelicool-xtask -- release\n\
                 cargo run --locked -p pelicool-xtask -- licenses PASTA"
            );
            Ok(())
        }
        _ => bail!("Uso: pelicool-xtask brand | release | licenses PASTA"),
    }
}
