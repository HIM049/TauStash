use super::TrayCommand;
use ksni::{Icon, MenuItem, TrayMethods, menu::StandardItem};
use rust_i18n::t;
use tokio::sync::mpsc::UnboundedSender;

pub type Tray = ksni::Handle<TrayService>;

pub struct TrayService {
    tx: UnboundedSender<TrayCommand>,
    icon: Icon,
}

impl ksni::Tray for TrayService {
    fn id(&self) -> String {
        "taustash".into()
    }

    fn title(&self) -> String {
        "TauStash".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: t!("tray.show").into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(TrayCommand::Show);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: t!("app_menu.quit").into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.tx.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Register the tray over D-Bus; fails when the desktop has no tray host.
pub fn init(tx: UnboundedSender<TrayCommand>) -> anyhow::Result<Tray> {
    let image = image::load_from_memory(include_bytes!("../../assets/app_icon.png"))?
        .resize(32, 32, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let (width, height) = image.dimensions();
    // StatusNotifierItem pixmaps are ARGB32 in network byte order.
    let mut data = image.into_vec();
    for pixel in data.chunks_exact_mut(4) {
        pixel.rotate_right(1);
    }
    let icon = Icon {
        width: width as i32,
        height: height as i32,
        data,
    };

    let service = TrayService { tx, icon };
    Ok(tokio::runtime::Handle::current().block_on(service.spawn())?)
}
