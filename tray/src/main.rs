//! ACP Portal tray: a small system-tray controller for the `acpd` host. It runs the same commands
//! you would type (setup, start, stop, pair) and shows the pairing QR code in your browser.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod host;
mod icon;
mod pairing_page;
mod tailscale;

use anyhow::{Context, Result};
use host::{Cli, HostState};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::{
    TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem},
};

/// How long a pairing code lives; matches the host.
const CODE_SECONDS: u32 = 60;

enum UserEvent {
    Status(HostState),
    /// Result of a menu action: an error message, or None when it worked.
    Done(Option<String>),
    Menu(MenuId),
}

fn settings_dir() -> PathBuf {
    acpd::config::default_directory()
}

fn label(state: &HostState) -> String {
    match state {
        HostState::NotSetUp => "ACP Portal: not set up yet".into(),
        HostState::Stopped => "ACP Portal: stopped".into(),
        HostState::Running { .. } => "ACP Portal: running".into(),
        HostState::Other(message) => format!("ACP Portal: {message}"),
    }
}

/// Writes the pairing page into a private temporary folder and opens it. The folder is removed after
/// the code has expired.
fn show_pairing(cli: &Cli) -> Result<()> {
    let pairing = cli.pair()?;
    let launcher = acpd::launcher::Launcher::load(&settings_dir().join("launcher.json"))?;
    let uri = acpd::pairing_input::pairing_uri(&pairing.address, &pairing.code, &launcher.name)?;
    let html = pairing_page::render(&uri, &pairing.code, &pairing.address, CODE_SECONDS)?;
    let dir = std::env::temp_dir().join(format!("acpd-pair-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let file = dir.join("pair.html");
    std::fs::write(&file, html)?;
    open::that(&file).context("open the pairing page in your browser")?;
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs((CODE_SECONDS + 60).into()));
        let _ = std::fs::remove_dir_all(dir);
    });
    Ok(())
}

/// Publishes the host over Tailscale and saves that address, so the phone can reach it.
fn setup_with_tailscale(cli: &Cli) -> Result<()> {
    let host = tailscale::status()?;
    let port = acpd::config::Config::load(&settings_dir().join("config.toml"))
        .map(|config| config.server.listen.port())
        .unwrap_or_else(|_| acpd::config::Config::default().server.listen.port());
    // Make sure the standard settings exist first, then publish and save the address.
    acpd::config::bootstrap_default_config(&settings_dir())?;
    tailscale::serve_background(port)?;
    cli.setup(&host.https_address(), &host.host_name)
}

fn spawn_action(
    proxy: &EventLoopProxy<UserEvent>,
    cli: &Arc<Cli>,
    action: impl FnOnce(&Cli) -> Result<()> + Send + 'static,
) {
    let (proxy, cli) = (proxy.clone(), cli.clone());
    std::thread::spawn(move || {
        let result = action(&cli).err().map(|error| error.to_string());
        let _ = proxy.send_event(UserEvent::Done(result));
        let _ = proxy.send_event(UserEvent::Status(cli.status(&settings_dir())));
    });
}

fn main() -> Result<()> {
    let cli = Arc::new(Cli::locate()?);
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id));
    }));
    // Poll the host every few seconds so the icon follows `acpd start/stop` typed elsewhere too.
    {
        let (proxy, cli) = (proxy.clone(), cli.clone());
        std::thread::spawn(move || {
            loop {
                if proxy
                    .send_event(UserEvent::Status(cli.status(&settings_dir())))
                    .is_err()
                {
                    break;
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        });
    }

    let status_item = MenuItem::new("ACP Portal", false, None);
    let problem_item = MenuItem::new("", false, None);
    let setup_item = MenuItem::new("Set up with Tailscale", true, None);
    let toggle_item = MenuItem::new("Start host", true, None);
    let pair_item = MenuItem::new("Pair a phone…", false, None);
    let folder_item = MenuItem::new("Open settings folder", true, None);
    let quit_item = MenuItem::new("Quit (the host keeps running)", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status_item,
        &problem_item,
        &PredefinedMenuItem::separator(),
        &setup_item,
        &toggle_item,
        &pair_item,
        &PredefinedMenuItem::separator(),
        &folder_item,
        &quit_item,
    ])?;
    let ids = (
        setup_item.id().clone(),
        toggle_item.id().clone(),
        pair_item.id().clone(),
        folder_item.id().clone(),
        quit_item.id().clone(),
    );

    let mut tray = None;
    let mut state = HostState::Stopped;
    let mut menu_holder = Some(menu);
    problem_item.set_text("");
    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            tao::event::Event::NewEvents(tao::event::StartCause::Init) => {
                // The tray icon must be created once the event loop is running (required on Linux).
                if let Some(menu) = menu_holder.take() {
                    tray = TrayIconBuilder::new()
                        .with_menu(Box::new(menu))
                        .with_tooltip("ACP Portal")
                        .with_icon(icon::icon(&state))
                        .build()
                        .ok();
                }
            }
            tao::event::Event::UserEvent(UserEvent::Status(next)) => {
                if next != state {
                    state = next;
                    status_item.set_text(label(&state));
                    toggle_item.set_text(if matches!(state, HostState::Running { .. }) {
                        "Stop host"
                    } else {
                        "Start host"
                    });
                    toggle_item.set_enabled(!matches!(state, HostState::NotSetUp));
                    pair_item.set_enabled(matches!(state, HostState::Running { .. }));
                    setup_item.set_enabled(true);
                    setup_item.set_text(if matches!(state, HostState::NotSetUp) {
                        "Set up with Tailscale"
                    } else {
                        "Set up again with Tailscale"
                    });
                    if let Some(tray) = &tray {
                        let _ = tray.set_icon(Some(icon::icon(&state)));
                        let _ = tray.set_tooltip(Some(label(&state)));
                    }
                }
            }
            tao::event::Event::UserEvent(UserEvent::Done(result)) => {
                problem_item.set_text(result.map(|m| format!("⚠ {m}")).unwrap_or_default());
            }
            tao::event::Event::UserEvent(UserEvent::Menu(id)) => {
                problem_item.set_text("");
                if id == ids.4 {
                    *control_flow = ControlFlow::Exit;
                } else if id == ids.0 {
                    spawn_action(&proxy, &cli, setup_with_tailscale);
                } else if id == ids.1 {
                    if matches!(state, HostState::Running { .. }) {
                        spawn_action(&proxy, &cli, |cli| cli.stop());
                    } else {
                        spawn_action(&proxy, &cli, |cli| cli.start());
                    }
                } else if id == ids.2 {
                    spawn_action(&proxy, &cli, show_pairing);
                } else if id == ids.3 {
                    let dir: &Path = &settings_dir();
                    let _ = std::fs::create_dir_all(dir);
                    let _ = open::that(dir);
                }
            }
            _ => {}
        }
    });
}
