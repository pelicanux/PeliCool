//! Enquadramento normalizado, compartilhado pelo editor e pelo conversor.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Framing {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}
impl Default for Framing {
    fn default() -> Self {
        Self {
            zoom: 1.,
            x: 0.5,
            y: 0.5,
        }
    }
}
impl Framing {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.zoom.is_finite()
                && (1.0..=8.0).contains(&self.zoom)
                && self.x.is_finite()
                && (0.0..=1.0).contains(&self.x)
                && self.y.is_finite()
                && (0.0..=1.0).contains(&self.y),
            "Enquadramento inválido."
        );
        Ok(())
    }
    /// Retângulo visível em pixels da origem. Nunca ultrapassa a imagem.
    pub fn rect(self, width: u32, height: u32, target_ratio: f32) -> (u32, u32, u32, u32) {
        let (w, h) = (width as f32, height as f32);
        let cw = w.min(h * target_ratio) / self.zoom;
        let ch = cw / target_ratio;
        let crop_w = (cw.round() as u32).clamp(1, width);
        let crop_h = (ch.round() as u32).clamp(1, height);
        let x = ((width - crop_w) as f32 * self.x).round() as u32;
        let y = ((height - crop_h) as f32 * self.y).round() as u32;
        (x, y, crop_w, crop_h)
    }
}
