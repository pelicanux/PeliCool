mod gui;

use clap::{Parser, Subcommand};
use pelicool::{
    device::{self, DisplayMode, Preset, RgbMode, Settings},
    drivers::{self, DriverModule},
    media::{Fit, Orientation},
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "PeliCool · Personalização de displays LCD e iluminação no Linux"
)]
struct Args {
    /// Escolhe um módulo registrado; sem esta opção, detecta pelo USB.
    #[arg(long, global = true)]
    driver: Option<String>,
    #[command(subcommand)]
    command: Option<Action>,
}

#[derive(Subcommand)]
enum Action {
    /// Lista módulos compilados, IDs USB e modelos validados. Não acessa o aparelho.
    Drivers,
    /// Exibe as licenças dos materiais incorporados, sem acessar o USB.
    Licenses,
    /// Instala o AppImage para o usuário atual e cria um atalho.
    Install {
        #[arg(long)]
        source: Option<PathBuf>,
    },
    /// Remove a instalação; --purge inclui configurações e cópias locais.
    Uninstall {
        #[arg(long)]
        purge: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Coleta um relatório local voluntário; nunca usa rede ou comandos USB.
    Diagnose {
        #[arg(long)]
        consent: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        include_descriptors: bool,
        #[arg(long)]
        allow_root: bool,
    },
    /// Instala/remove a regra udev. Requer autenticação administrativa.
    Permissions {
        #[arg(value_parser=["install","remove"],default_value="install")]
        action: String,
    },
    /// Abre a interface gráfica. Não altera o aparelho ao iniciar.
    Gui {
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long, hide = true)]
        screenshot: Option<PathBuf>,
        /// Inicia com o tema escuro.
        #[arg(long, conflicts_with = "light")]
        dark: bool,
        /// Inicia com tema claro, sem sobrescrever a preferência salva.
        #[arg(long)]
        light: bool,
        /// Mantém a janela acessível no painel/atividades ao minimizar, sem usar bandeja.
        #[arg(long)]
        no_tray: bool,
        /// Inicia minimizado; sem bandeja, mantém a janela no painel/atividades.
        #[arg(long)]
        start_minimized: bool,
        #[arg(long, hide = true)]
        preview_close: bool,
        #[arg(long, hide = true, requires = "screenshot")]
        preview_udev: bool,
        #[arg(long, hide = true, value_enum, requires = "screenshot")]
        preview_progress: Option<gui::ProgressPreview>,
        #[arg(long, hide = true, value_enum, default_value_t)]
        view: gui::Page,
        #[arg(long, value_enum, default_value_t)]
        mode: DisplayMode,
    },
    /// Consulta o estado USB sem alterar tela, iluminação ou mídia.
    Probe,
    /// Mostra leituras reais do Linux sem acessar o USB.
    Sensors,
    /// Volta à mídia que já está armazenada, sem apagar ou reenviar arquivos.
    ShowMedia {
        #[arg(long, value_enum, default_value_t)]
        rgb: RgbMode,
        #[arg(long)]
        refresh_openrgb: bool,
        #[arg(long)]
        rgb_profile: Option<PathBuf>,
        #[arg(long)]
        live_sdk: bool,
    },
    /// Salva a coleção completa de até cinco imagens e cinco GIFs.
    SaveCollection {
        #[arg(required=true,num_args=1..=10)]
        files: Vec<PathBuf>,
        #[arg(long, required = true)]
        replace_media: bool,
    },
    /// Alterna entre os itens da coleção salva sem novo upload.
    Slideshow {
        #[arg(long, default_value_t = 20)]
        seconds: u64,
        #[arg(long, default_value_t = 5)]
        interval: u8,
    },
    /// Seleciona uma mídia já salva por este aplicativo, sem upload.
    Select {
        #[arg(long)]
        gif: bool,
        #[arg(long, default_value_t = 0)]
        index: u8,
    },
    /// Ativa presets nativos e transmite sensores a cada segundo.
    Monitor {
        #[arg(long, value_enum, default_value_t=DisplayMode::Monitoring)]
        mode: DisplayMode,
        /// 0 frequência, 1 hora, 2 bomba, 3 fan, 4 fan+bomba, 5 temperatura.
        #[arg(long, default_value_t = 5)]
        main: u8,
        /// 0 normal, 1 anel, 2 cronômetro.
        #[arg(long, default_value_t = 0)]
        style: u8,
        /// 0 tensões, 1 sistema, 2 núcleos.
        #[arg(long, default_value_t = 1)]
        auxiliary: u8,
        /// Encerra após N segundos; 0 mantém a transmissão até encerrar o processo.
        #[arg(long, default_value_t = 0)]
        seconds: u64,
    },
    /// Converte e valida uma imagem/GIF sem acessar USB.
    Prepare {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        orientation: Orientation,
        #[arg(long, value_enum, default_value_t)]
        fit: Fit,
        #[arg(long)]
        preview: Option<PathBuf>,
    },
    /// Substitui a mídia armazenada no cooler pelo arquivo escolhido.
    Upload {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t)]
        orientation: Orientation,
        #[arg(long, value_enum, default_value_t)]
        fit: Fit,
        #[arg(long, value_enum, default_value_t)]
        rgb: RgbMode,
        #[arg(long, default_value_t = 80)]
        brightness: u8,
        #[arg(long, default_value_t = 0)]
        rotation: u16,
        /// Apaga todas as imagens/GIFs armazenados antes do envio.
        #[arg(long, required = true)]
        replace_media: bool,
    },
}

fn execute(args: Args) -> anyhow::Result<()> {
    if matches!(&args.command, Some(Action::Licenses)) {
        println!("PeliCool\n{}", include_str!("../LICENSE"));
        println!(
            "Protocolo adaptado do Mystique Control\n{}",
            include_str!("../LICENSES/gpteixeira-MIT.txt")
        );
        return Ok(());
    }
    match &args.command {
        Some(Action::Install { source }) => {
            let source = source
                .clone()
                .or_else(|| std::env::var_os("APPIMAGE").map(PathBuf::from))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Use o AppImage ou informe --source /caminho/PeliCool.AppImage."
                    )
                })?;
            let paths = pelicool::storage::paths::AppPaths::discover()?;
            pelicool::installation::install(&paths, &source)?;
            println!("PeliCool instalado em {}", paths.installation.display());
            return Ok(());
        }
        Some(Action::Uninstall { purge, yes }) => {
            if !yes {
                use std::io::{IsTerminal, Write};
                anyhow::ensure!(
                    std::io::stdin().is_terminal(),
                    "Para confirmar em scripts, use --yes; --purge também remove configurações e mídias locais."
                );
                println!(
                    "Remover instalação, atalho e autostart do PeliCool? {}",
                    if *purge {
                        "Também apagará configurações, diagnósticos e cópias locais de mídia."
                    } else {
                        "Configurações e mídias locais serão mantidas."
                    }
                );
                print!("Digite DESINSTALAR: ");
                std::io::stdout().flush()?;
                let mut answer = String::new();
                std::io::stdin().read_line(&mut answer)?;
                anyhow::ensure!(answer.trim() == "DESINSTALAR", "Desinstalação cancelada.");
            }
            pelicool::installation::uninstall(
                &pelicool::storage::paths::AppPaths::discover()?,
                *purge,
            )?;
            println!("PeliCool desinstalado.");
            return Ok(());
        }
        Some(Action::Diagnose {
            consent,
            output,
            include_descriptors,
            allow_root,
        }) => {
            let mut command = pelicool::diagnostics::command();
            if *consent {
                command.arg("--consent");
            }
            if *allow_root {
                command.arg("--allow-root");
            }
            if *include_descriptors {
                command.arg("--include-descriptors");
            }
            if let Some(output) = output {
                command.arg("--output").arg(output);
            }
            anyhow::ensure!(
                command.status()?.success(),
                "O diagnóstico não foi concluído."
            );
            return Ok(());
        }
        Some(Action::Permissions { action }) => {
            let action = match action.as_str() {
                "install" => pelicool::permissions::Action::Install,
                "remove" => pelicool::permissions::Action::Remove,
                _ => anyhow::bail!("Use install ou remove."),
            };
            println!("{}", pelicool::permissions::apply(action)?);
            return Ok(());
        }
        _ => {}
    }
    drivers::validate_registry(drivers::REGISTERED)?;
    if matches!(&args.command, Some(Action::Drivers)) {
        for module in drivers::REGISTERED {
            println!("{} · {}", module.info.id, module.info.name);
            println!(
                "  USB: {}",
                module
                    .info
                    .usb
                    .ids
                    .iter()
                    .map(|id| format!("{:04x}:{:04x}", id.vendor, id.product))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            println!("  Validado: {}", module.info.tested_models.join(", "));
            println!("  Modos: {:?}", module.info.capabilities.modes);
        }
        return Ok(());
    }
    let module = if matches!(&args.command, Some(Action::Sensors)) {
        drivers::by_id(args.driver.as_deref().unwrap_or(drivers::mystique::ID))?
    } else {
        drivers::resolve(args.driver.as_deref())?
    };
    match args.command {
        Some(
            Action::Drivers
            | Action::Licenses
            | Action::Install { .. }
            | Action::Uninstall { .. }
            | Action::Diagnose { .. }
            | Action::Permissions { .. },
        ) => unreachable!("handled before device detection"),
        None => gui::run(gui::Options {
            driver: Some(module.info.id.into()),
            ..Default::default()
        }),
        Some(Action::Gui {
            file,
            screenshot,
            dark,
            light,
            no_tray,
            start_minimized,
            preview_close,
            preview_udev,
            preview_progress,
            view,
            mode,
        }) => gui::run(gui::Options {
            driver: Some(module.info.id.into()),
            file,
            screenshot,
            dark: if dark {
                Some(true)
            } else if light {
                Some(false)
            } else {
                None
            },
            page: view,
            mode,
            no_tray,
            start_minimized,
            preview_close,
            preview_udev,
            preview_progress,
        }),
        Some(Action::Sensors) => {
            let mut reader = pelicool::sensors::Reader::default();
            std::thread::sleep(std::time::Duration::from_millis(250));
            println!("{:#?}", reader.sample());
            Ok(())
        }
        Some(Action::ShowMedia {
            rgb,
            refresh_openrgb,
            rgb_profile,
            live_sdk,
        }) => {
            let cache = pelicool::collection::Cache::for_driver(module.info.id)?;
            let _lock = cache.lock()?;
            let mut catalog = cache.load()?;
            let settings = Settings {
                rgb,
                ..catalog.settings
            };
            anyhow::ensure!(
                !catalog.rotation_pending(module.info, settings.rotation),
                "A orientação precisa ser atualizada. Aplique tela e LEDs na interface primeiro."
            );
            module.open()?.apply(settings)?;
            if refresh_openrgb && rgb == RgbMode::Motherboard {
                std::thread::sleep(std::time::Duration::from_millis(200));
                let result = if live_sdk {
                    pelicool::openrgb::refresh_motherboard()?
                } else {
                    pelicool::openrgb::apply_profile(rgb_profile.as_deref())?
                };
                println!("OpenRGB: {result}");
            }
            catalog.settings = settings;
            if let Some(path) = rgb_profile {
                catalog.rgb_profile = Some(path);
            }
            catalog.openrgb_live = live_sdk;
            cache.save(&catalog)?;
            println!("Modo mídia selecionado. Nenhum arquivo foi apagado.");
            Ok(())
        }
        Some(Action::SaveCollection {
            files,
            replace_media,
        }) => {
            anyhow::ensure!(replace_media, "Confirme a substituição da coleção.");
            let cache = pelicool::collection::Cache::for_driver(module.info.id)?;
            let _lock = cache.lock()?;
            let mut catalog = pelicool::collection::Catalog::default();
            for file in files {
                let prepared = module.prepare(&file, Orientation::Auto, Fit::Cover)?;
                catalog.entries.push(cache.stage(
                    &file,
                    &prepared,
                    Orientation::Auto,
                    Fit::Cover,
                )?);
            }
            save_collection(module, &cache, &mut catalog, Settings::default())?;
            println!("Coleção salva com {} item(ns).", catalog.entries.len());
            Ok(())
        }
        Some(Action::Slideshow { seconds, interval }) => {
            gui::play_collection(seconds, interval, module)
        }
        Some(Action::Select { gif, index }) => {
            let cache = pelicool::collection::Cache::for_driver(module.info.id)?;
            let _lock = cache.lock()?;
            let catalog = cache.load()?;
            anyhow::ensure!(
                !catalog.rotation_pending(module.info, catalog.settings.rotation),
                "A orientação precisa ser atualizada pela interface antes da seleção sem upload."
            );
            let slot = pelicool::collection::Slot {
                animated: gif,
                index,
            };
            slot.validate()?;
            anyhow::ensure!(
                catalog.valid && catalog.saved.iter().any(|s| s.slot == slot),
                "Slot não confirmado na coleção local."
            );
            let mut controller = module.open()?;
            controller.apply(catalog.settings)?;
            controller.select_media(slot)?;
            println!(
                "Slot {} {} selecionado sem upload.",
                index,
                if gif { "GIF" } else { "imagem" }
            );
            Ok(())
        }
        Some(Action::Monitor {
            mode,
            main,
            style,
            auxiliary,
            seconds,
        }) => {
            let preset = Preset {
                mode,
                main,
                style,
                auxiliary,
                ..Default::default()
            };
            let mut reader = pelicool::sensors::Reader::default();
            std::thread::sleep(std::time::Duration::from_millis(250));
            let mut snapshot = reader.sample();
            preset.check_sources(&snapshot)?;
            let settings = Settings {
                rgb: if module
                    .info
                    .capabilities
                    .rgb_modes
                    .contains(&RgbMode::Internal)
                {
                    RgbMode::Internal
                } else {
                    module
                        .info
                        .capabilities
                        .rgb_modes
                        .first()
                        .copied()
                        .unwrap_or_default()
                },
                ..Default::default()
            };
            let mut controller = module.open()?;
            controller.configure_preset(settings, preset, true)?;
            let start = std::time::Instant::now();
            let mut synced = start;
            loop {
                preset.check_sources(&snapshot)?;
                controller.update_sensors(&snapshot)?;
                println!(
                    "CPU {:?} °C · uso {:?}% · RAM {:?}% · {:?} MHz",
                    snapshot.cpu_temp, snapshot.cpu_usage, snapshot.ram_usage, snapshot.cpu_mhz
                );
                if seconds > 0 && start.elapsed() >= std::time::Duration::from_secs(seconds) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
                snapshot = reader.sample();
                if synced.elapsed() >= std::time::Duration::from_secs(30) {
                    controller.sync_clock()?;
                    synced = std::time::Instant::now();
                }
            }
            Ok(())
        }
        Some(Action::Probe) => {
            println!("{}", module.open()?.probe()?);
            Ok(())
        }
        Some(Action::Prepare {
            file,
            orientation,
            fit,
            preview,
        }) => {
            let media = module.prepare(&file, orientation, fit)?;
            println!(
                "{}; {} quadro(s); {} bytes JPEG; duração {} ms; buffer 480×640.",
                if media.animated { "GIF" } else { "Imagem" },
                media.frames.len(),
                media.byte_len(),
                media.duration_ms
            );
            if let Some(path) = preview {
                media.frames[0].preview.save(&path)?;
                println!("Prévia: {}", path.display());
            }
            Ok(())
        }
        Some(Action::Upload {
            file,
            orientation,
            fit,
            rgb,
            brightness,
            rotation,
            replace_media,
        }) => {
            let settings = Settings {
                rgb,
                brightness,
                rotation,
                ..Default::default()
            };
            settings.validate()?;
            let media = module.prepare(&file, orientation, fit)?;
            anyhow::ensure!(replace_media, "Confirme a substituição da coleção.");
            let cache = pelicool::collection::Cache::for_driver(module.info.id)?;
            let _lock = cache.lock()?;
            let entry = cache.stage(&file, &media, orientation, fit)?;
            let mut catalog = pelicool::collection::Catalog {
                entries: vec![entry],
                ..Default::default()
            };
            save_collection(module, &cache, &mut catalog, settings)?;
            println!("Transferência concluída. Confira o resultado na tela física do cooler.");
            Ok(())
        }
    }
}

fn save_collection(
    module: &'static DriverModule,
    cache: &pelicool::collection::Cache,
    catalog: &mut pelicool::collection::Catalog,
    settings: Settings,
) -> anyhow::Result<()> {
    let mut controller = module.open()?;
    catalog.settings = settings;
    let result = pelicool::collection::sync::save(
        cache,
        &mut controller,
        catalog,
        settings,
        None,
        |_| {},
        |_| {},
    )?;
    if let Some(warning) = result.cleanup_warning {
        eprintln!("{warning}");
    }
    Ok(())
}

fn main() -> std::process::ExitCode {
    match execute(Args::parse()) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Erro: {}", device::describe_error(&error));
            std::process::ExitCode::FAILURE
        }
    }
}
