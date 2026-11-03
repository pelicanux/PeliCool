//! Exporta a marca a partir do SVG mestre aprovado, sem refazer sua vetorização.
use anyhow::{Context, Result, ensure};
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg::{Options, Tree, TreeParsing},
};
use std::{fs, path::Path};

struct Artwork<'a> {
    width: f32,
    height: f32,
    groups: Vec<&'a str>,
    left: f32,
    top: f32,
}

impl<'a> Artwork<'a> {
    fn parse(svg: &'a str) -> Result<Self> {
        let document = roxmltree::Document::parse(svg)?;
        let root = document.root_element();
        ensure!(root.has_tag_name("svg"), "A marca precisa ser um SVG");
        ensure!(
            !root.descendants().skip(1).any(|node| {
                node.is_element()
                    && matches!(node.tag_name().name(), "svg" | "image" | "text" | "use")
            }),
            "A marca deve usar contornos próprios, sem SVG aninhado, bitmap, texto ou referências"
        );
        let viewbox: Vec<f32> = root
            .attribute("viewBox")
            .context("A marca precisa de viewBox")?
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        ensure!(
            viewbox.len() == 4
                && viewbox.iter().all(|value| value.is_finite())
                && viewbox[2] > 0.
                && viewbox[3] > 0.,
            "viewBox inválido"
        );
        let groups: Vec<_> = root
            .children()
            .filter(|node| node.has_tag_name("g"))
            .map(|node| &svg[node.range()])
            .collect();
        ensure!(!groups.is_empty(), "A marca não contém grupos vetoriais");
        Ok(Self {
            left: viewbox[0],
            top: viewbox[1],
            width: viewbox[2],
            height: viewbox[3],
            groups,
        })
    }

    fn icon(&self) -> String {
        let scale = 832. / self.width.max(self.height);
        let x = (1024. - self.width * scale) / 2.;
        let y = (1024. - self.height * scale) / 2.;
        // Uma só raiz SVG: QtSvg/KDE descartava o desenho com viewport aninhada.
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 1024 1024\" role=\"img\" aria-labelledby=\"title desc\">\n\
             <title id=\"title\">Ícone PeliCool</title>\n\
             <desc id=\"desc\">Marca PeliCool sobre fundo claro com cantos arredondados.</desc>\n\
             <rect x=\"16\" y=\"16\" width=\"992\" height=\"992\" rx=\"224\" fill=\"#e5f6f1\"/>\n\
             <g transform=\"translate({x} {y}) scale({scale}) translate({left} {top})\">\n{groups}\n</g>\n</svg>\n",
            left = -self.left,
            top = -self.top,
            groups = self.groups.join("\n"),
        )
    }
}

fn render(svg: &str, width: u32, height: u32, destination: &Path) -> Result<()> {
    let tree = Tree::from_str(svg, &Options::default())?;
    let renderer = resvg::Tree::from_usvg(&tree);
    let mut pixels = Pixmap::new(width, height).context("Dimensões de exportação inválidas")?;
    renderer.render(
        Transform::from_scale(
            width as f32 / renderer.size.width(),
            height as f32 / renderer.size.height(),
        ),
        &mut pixels.as_mut(),
    );
    pixels.save_png(destination)?;
    Ok(())
}

pub fn export(root: &Path) -> Result<()> {
    let assets = root.join("assets");
    let master = fs::read_to_string(assets.join("pelicool.svg"))?;
    let artwork = Artwork::parse(&master)?;
    let icon = artwork.icon();
    fs::write(assets.join("pelicool-wordmark.svg"), &master)?;
    fs::write(assets.join("pelicool-icon.svg"), &icon)?;
    render(
        &master,
        1024,
        (1024. * artwork.height / artwork.width).round() as u32,
        &assets.join("pelicool.png"),
    )?;
    render(&icon, 256, 256, &assets.join("pelicool-icon.png"))?;
    render(&icon, 64, 64, &assets.join("pelicool-tray.png"))?;
    println!("Marca e ícones exportados a partir de assets/pelicool.svg.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_icon_keeps_a_single_viewport_and_visible_colored_paths() {
        let artwork = Artwork::parse(include_str!("../../assets/pelicool.svg")).unwrap();
        let icon = artwork.icon();
        let document = roxmltree::Document::parse(&icon).unwrap();
        assert_eq!(
            document
                .descendants()
                .filter(|node| node.has_tag_name("svg"))
                .count(),
            1
        );
        let tree = Tree::from_str(&icon, &Options::default()).unwrap();
        let renderer = resvg::Tree::from_usvg(&tree);
        let mut pixels = Pixmap::new(48, 48).unwrap();
        renderer.render(
            Transform::from_scale(48. / 1024., 48. / 1024.),
            &mut pixels.as_mut(),
        );
        let colored = pixels
            .pixels()
            .iter()
            .filter(|pixel| {
                pixel.alpha() > 128
                    && pixel.red() < 40
                    && pixel.green() > 80
                    && pixel.green() < 200
                    && pixel.blue() > 80
                    && pixel.blue() < 200
            })
            .count();
        assert!(
            colored > 100,
            "Ícone sem pelicano visível: {colored} pixels"
        );
    }

    #[test]
    fn nested_viewports_and_external_content_are_rejected() {
        for child in ["svg", "image", "text", "use"] {
            let svg = format!("<svg viewBox=\"0 0 10 10\"><g/><{child}/></svg>");
            assert!(Artwork::parse(&svg).is_err());
        }
    }
}
