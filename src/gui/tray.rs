use super::brand;
use eframe::egui;
use ksni::blocking::TrayMethods;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Action {
    Show,
    Minimize,
    Quit,
}

struct PeliTray {
    actions: Sender<Action>,
    ctx: egui::Context,
    available: Arc<AtomicBool>,
    icon: ksni::Icon,
}

impl PeliTray {
    fn send(&self, action: Action) {
        let _ = self.actions.send(action);
        self.ctx.request_repaint();
    }
}

impl ksni::Tray for PeliTray {
    fn id(&self) -> String {
        "pelicool".into()
    }

    fn title(&self) -> String {
        brand::NAME.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: brand::NAME.into(),
            description: "Controle do MYSTIQUE · clique para abrir".into(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Action::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, StandardItem};
        vec![
            StandardItem {
                label: "Abrir PeliCool".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Action::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Minimizar na bandeja".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Action::Minimize)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Sair do PeliCool".into(),
                activate: Box::new(|tray: &mut Self| tray.send(Action::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.available.store(false, Ordering::Release);
        self.ctx.request_repaint();
        // Restart through the normal, checked spawn path. Do not assume that
        // a watcher without a registered host can display a recoverable icon.
        false
    }
}

/// Registration runs off the UI thread. A successful spawn confirms both a
/// watcher and a host; neither desktop names nor environment guesses suffice.
pub(super) struct Service {
    pub(super) actions: Receiver<Action>,
    available: Arc<AtomicBool>,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Service {
    pub(super) fn new(ctx: egui::Context, enabled: bool) -> Self {
        let (actions, incoming) = mpsc::channel();
        let (stop, stopped) = mpsc::channel();
        let available = Arc::new(AtomicBool::new(false));
        let thread = enabled.then(|| {
            let available = available.clone();
            thread::spawn(move || {
                let image =
                    image::load_from_memory(include_bytes!("../../assets/pelicool-tray.png"))
                        .expect("embedded tray PNG must be valid")
                        .to_rgba8();
                let icon = ksni::Icon {
                    width: image.width() as i32,
                    height: image.height() as i32,
                    data: image
                        .pixels()
                        .flat_map(|p| [p[3], p[0], p[1], p[2]])
                        .collect(),
                };
                let mut handle: Option<ksni::blocking::Handle<PeliTray>> = None;
                let mut retry = Instant::now();
                loop {
                    if stopped.try_recv().is_ok() {
                        break;
                    }
                    if handle.as_ref().is_some_and(|handle| handle.is_closed()) {
                        handle = None;
                        available.store(false, Ordering::Release);
                        ctx.request_repaint();
                    }
                    if handle.is_none() && Instant::now() >= retry {
                        handle = PeliTray {
                            actions: actions.clone(),
                            ctx: ctx.clone(),
                            available: available.clone(),
                            icon: icon.clone(),
                        }
                        .spawn()
                        .ok();
                        available.store(
                            handle.as_ref().is_some_and(|handle| !handle.is_closed()),
                            Ordering::Release,
                        );
                        ctx.request_repaint();
                        retry = Instant::now() + Duration::from_secs(3);
                    }
                    if stopped.recv_timeout(Duration::from_millis(100)).is_ok() {
                        break;
                    }
                }
                available.store(false, Ordering::Release);
                if let Some(handle) = handle {
                    handle.shutdown().wait();
                }
            })
        });
        Self {
            actions: incoming,
            available,
            stop,
            thread,
        }
    }

    pub(super) fn available(&self) -> bool {
        self.available.load(Ordering::Acquire)
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, sync::Mutex};
    use zbus::{
        blocking::{Connection, Proxy, connection::Builder},
        zvariant::{OwnedValue, Value},
    };

    struct Watcher {
        host: Arc<AtomicBool>,
        registered: Arc<Mutex<Vec<String>>>,
        checked_without_host: Arc<AtomicBool>,
    }

    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, service: &str) {
            self.registered.lock().unwrap().push(service.to_owned());
        }

        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            let host = self.host.load(Ordering::Acquire);
            if !host {
                self.checked_without_host.store(true, Ordering::Release);
            }
            host
        }
    }

    fn wait_for(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(6);
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for tray state"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "run with PELICOOL_PRIVATE_BUS_TEST=1 dbus-run-session -- cargo test --bin pelicool private_bus -- --ignored"]
    fn private_bus_checks_host_menu_actions_and_watcher_loss() {
        assert_eq!(
            std::env::var("PELICOOL_PRIVATE_BUS_TEST").as_deref(),
            Ok("1")
        );
        let host = Arc::new(AtomicBool::new(false));
        let registered = Arc::new(Mutex::new(vec![]));
        let checked_without_host = Arc::new(AtomicBool::new(false));
        let watcher = Builder::session()
            .unwrap()
            .name("org.kde.StatusNotifierWatcher")
            .unwrap()
            .serve_at(
                "/StatusNotifierWatcher",
                Watcher {
                    host: host.clone(),
                    registered: registered.clone(),
                    checked_without_host: checked_without_host.clone(),
                },
            )
            .unwrap()
            .build()
            .unwrap();
        let service = Service::new(egui::Context::default(), true);
        wait_for(|| checked_without_host.load(Ordering::Acquire));
        assert!(
            !service.available(),
            "a watcher without a host must not enable hiding"
        );
        host.store(true, Ordering::Release);
        wait_for(|| service.available());
        let name = registered.lock().unwrap().last().unwrap().clone();
        let client = Connection::session().unwrap();
        let item = Proxy::new(
            &client,
            name.as_str(),
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
        )
        .unwrap();
        assert_eq!(item.get_property::<String>("Id").unwrap(), "pelicool");
        assert_eq!(item.get_property::<String>("Title").unwrap(), "PeliCool");
        let pixmaps: Vec<(i32, i32, Vec<u8>)> = item.get_property("IconPixmap").unwrap();
        assert_eq!(
            (pixmaps[0].0, pixmaps[0].1, pixmaps[0].2.len()),
            (64, 64, 64 * 64 * 4)
        );
        item.call::<_, _, ()>("Activate", &(0i32, 0i32)).unwrap();
        assert_eq!(
            service
                .actions
                .recv_timeout(Duration::from_secs(2))
                .unwrap(),
            Action::Show
        );
        let menu_path: zbus::zvariant::OwnedObjectPath = item.get_property("Menu").unwrap();
        let menu = Proxy::new(
            &client,
            name.as_str(),
            menu_path.as_str(),
            "com.canonical.dbusmenu",
        )
        .unwrap();
        type Node = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);
        let (_, root): (u32, Node) = menu
            .call("GetLayout", &(0i32, -1i32, Vec::<String>::new()))
            .unwrap();
        let mut found = vec![];
        for child in root.2 {
            let (id, props, _): Node = child.try_into().unwrap();
            let Some(label) = props.get("label") else {
                continue;
            };
            let label = <&str>::try_from(label).unwrap();
            let expected = match label {
                "Abrir PeliCool" => Action::Show,
                "Minimizar na bandeja" => Action::Minimize,
                "Sair do PeliCool" => Action::Quit,
                other => panic!("unexpected menu item: {other}"),
            };
            menu.call::<_, _, ()>("Event", &(id, "clicked", Value::from(0i32), 0u32))
                .unwrap();
            assert_eq!(
                service
                    .actions
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap(),
                expected
            );
            found.push(expected);
        }
        assert_eq!(found, [Action::Show, Action::Minimize, Action::Quit]);
        let mut state = super::super::closing::WindowState::Asking;
        assert_eq!(
            state.choose(
                super::super::closing::Choice::Minimize,
                service.available(),
                false
            ),
            super::super::closing::Effect::Hide
        );
        watcher
            .release_name("org.kde.StatusNotifierWatcher")
            .unwrap();
        wait_for(|| !service.available());
        assert_eq!(
            state.reconcile_tray(service.available()),
            super::super::closing::Effect::Show
        );
        watcher
            .request_name("org.kde.StatusNotifierWatcher")
            .unwrap();
        wait_for(|| service.available());
    }
}
