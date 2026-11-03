use eframe::egui;

pub(super) const NAME: &str = "PeliCool";
const APP_ICON: &[u8] = include_bytes!("../../assets/pelicool-icon.png");

pub(super) fn image() -> image::RgbaImage {
    image::load_from_memory(APP_ICON)
        .expect("embedded PeliCool icon must be a valid PNG")
        .to_rgba8()
}

pub(super) fn logo() -> egui::ImageSource<'static> {
    egui::include_image!("../../assets/pelicool.svg")
}

pub(super) fn window_icon() -> egui::IconData {
    let image = image();
    egui::IconData {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    }
}
