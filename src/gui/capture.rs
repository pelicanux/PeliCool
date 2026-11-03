//! Capturas da interface para documentação, sem comandos USB.
use super::{App, WindowState};
use eframe::egui;

impl App {
    pub(super) fn screenshots(&mut self, ctx: &egui::Context) {
        if let Some(path) = &self.screenshot {
            let screenshot = ctx.input(|input| {
                input.events.iter().find_map(|event| {
                    if let egui::Event::Screenshot { image, .. } = event {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
            });
            if let Some(image) = screenshot {
                let bytes: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();
                match image::save_buffer(
                    path,
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        self.window_state = WindowState::Exiting;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    Err(error) => {
                        self.error = true;
                        self.status = error.to_string();
                    }
                }
            }
        }
    }
}
