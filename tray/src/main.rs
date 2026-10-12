//! ACP Portal tray: a small system-tray controller for the `acpd` host. It runs the same commands
//! you would type (setup, start, stop, pair), shows the pairing QR code in your browser, and can
//! download and install the host itself, so the tray works on its own.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod host;
mod icon;
mod install;
mod pairing_page;
mod tailscale;
mod uninstall;

use anyhow::{Context, Result};
use host::{Cli, Control, HostState};
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
    /// A message for the line under the status (progress or success).
    Note(String),
    /// Result of a menu action: an error message, or None when it worked.
    Done(Option<String>),
    Menu(MenuId),
    /// The line about Tailscale, refreshed in the background.
    Tailscale(String),
    /// The uninstall finished: the tray quits, or shows what went wrong.
    Uninstalled(Option<String>),
}

const UNINSTALL_LABEL: &str = "Uninstall ACP Portal…";
const PURGE_LABEL: &str = "Uninstall and delete my settings…";
/// How long the second click may take to confirm an uninstall.
const CONFIRM_SECONDS: u64 = 10;

fn settings_dir() -> PathBuf {
    acpd::config::default_directory()
}

fn label(state: &HostState) -> String {
    match state {
        HostState::NotInstalled => "ACP Portal: host not installed".into(),
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

/// Waits for Tailscale to be connected (reconnecting it if it was switched off), publishes the host over
/// it and saves that address, so the phone can reach it. Progress shows in the menu as it goes.
fn setup_with_tailscale(cli: &Cli, proxy: &EventLoopProxy<UserEvent>) -> Result<()> {
    let note = |text: &str| {
        let _ = proxy.send_event(UserEvent::Note(text.to_string()));
    };
    note("Checking Tailscale…");
    let host = tailscale::ensure_ready(note)?;
    let port = acpd::config::Config::load(&settings_dir().join("config.toml"))
        .map(|config| config.server.listen.port())
        .unwrap_or_else(|_| acpd::config::Config::default().server.listen.port());
    // Make sure the standard settings exist first, then publish and save the address.
    acpd::config::bootstrap_default_config(&settings_dir())?;
    note("Publishing the host over Tailscale…");
    tailscale::serve_background(port)?;
    cli.setup(&host.https_address(), &host.host_name)?;
    note(&format!("✓ Reachable at {}", host.https_address()));
    Ok(())
}

/// Runs a host action off the UI thread, then reports the result and the new state.
fn spawn_action(
    proxy: &EventLoopProxy<UserEvent>,
    control: &Arc<Control>,
    action: impl FnOnce(&Cli) -> Result<()> + Send + 'static,
) {
    let (proxy, control) = (proxy.clone(), control.clone());
    std::thread::spawn(move || {
        let result = control.with(action).err().map(|error| error.to_string());
        let _ = proxy.send_event(UserEvent::Done(result));
        let _ = proxy.send_event(UserEvent::Status(control.status(&settings_dir())));
    });
}

/// Downloads the latest host from GitHub and installs it; the tray then uses it straight away.
fn spawn_install(proxy: &EventLoopProxy<UserEvent>, control: &Arc<Control>) {
    let (proxy, control) = (proxy.clone(), control.clone());
    std::thread::spawn(move || {
        let _ = proxy.send_event(UserEvent::Note("Downloading the host from GitHub…".into()));
        match install::download_latest() {
            Ok((path, tag)) => {
                control.refresh();
                let _ = proxy.send_event(UserEvent::Note(format!(
                    "✓ Installed acpd {tag} in {}",
                    path.display()
                )));
            }
            Err(error) => {
                let _ = proxy.send_event(UserEvent::Done(Some(error.to_string())));
            }
        }
        let _ = proxy.send_event(UserEvent::Status(control.status(&settings_dir())));
    });
}

/// Stops the host, removes the programs (and, when asked, the settings), then lets the tray quit.
fn spawn_uninstall(proxy: &EventLoopProxy<UserEvent>, control: &Arc<Control>, purge: bool) {
    let (proxy, control) = (proxy.clone(), control.clone());
    std::thread::spawn(move || {
        let _ = proxy.send_event(UserEvent::Note("Uninstalling…".into()));
        let cli = control.get();
        let result = uninstall::uninstall(cli.as_deref(), purge)
            .err()
            .map(|error| error.to_string());
        let _ = proxy.send_event(UserEvent::Uninstalled(result));
    });
}

fn main() -> Result<()> {
    let control = Arc::new(Control::new());
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id));
    }));
    // Poll the host every few seconds so the icon follows `acpd start/stop` typed elsewhere too, and
    // notices an `acpd` that was copied next to the tray or installed since the last check.
    {
        let (proxy, control) = (proxy.clone(), control.clone());
        std::thread::spawn(move || {
            loop {
                if control.get().is_none() {
                    control.refresh();
                }
                if proxy
                    .send_event(UserEvent::Status(control.status(&settings_dir())))
                    .is_err()
                {
                    break;
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        });
    }

    // Tailscale is checked on its own, slower rhythm (it is a separate program); this only reads its status.
    {
        let proxy = proxy.clone();
        std::thread::spawn(move || {
            loop {
                if proxy
                    .send_event(UserEvent::Tailscale(tailscale::detect().line()))
                    .is_err()
                {
                    break;
                }
                std::thread::sleep(Duration::from_secs(20));
            }
        });
    }

    let status_item = MenuItem::new("ACP Portal", false, None);
    let tailscale_item = MenuItem::new("Tailscale: checking…", false, None);
    let problem_item = MenuItem::new("", false, None);
    let install_item = MenuItem::new("Download host (acpd)…", true, None);
    let setup_item = MenuItem::new("Set up with Tailscale", true, None);
    let toggle_item = MenuItem::new("Start host", true, None);
    let pair_item = MenuItem::new("Pair a phone…", false, None);
    let folder_item = MenuItem::new("Open settings folder", true, None);
    let uninstall_item = MenuItem::new(UNINSTALL_LABEL, true, None);
    let purge_item = MenuItem::new(PURGE_LABEL, true, None);
    let quit_item = MenuItem::new("Quit (the host keeps running)", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status_item,
        &tailscale_item,
        &problem_item,
        &PredefinedMenuItem::separator(),
        &install_item,
        &setup_item,
        &toggle_item,
        &pair_item,
        &PredefinedMenuItem::separator(),
        &folder_item,
        &PredefinedMenuItem::separator(),
        &uninstall_item,
        &purge_item,
        &quit_item,
    ])?;
    let (uninstall_id, purge_id) = (uninstall_item.id().clone(), purge_item.id().clone());
    let (install_id, setup_id, toggle_id, pair_id, folder_id, quit_id) = (
        install_item.id().clone(),
        setup_item.id().clone(),
        toggle_item.id().clone(),
        pair_item.id().clone(),
        folder_item.id().clone(),
        quit_item.id().clone(),
    );

    let mut tray = None;
    let mut state = HostState::Stopped;
    let mut menu_holder = Some(menu);
    // An uninstall needs a second click within a few seconds: (delete settings too, when asked).
    let mut confirming: Option<(bool, std::time::Instant)> = None;
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
            tao::event::Event::UserEvent(UserEvent::Uninstalled(result)) => match result {
                None => *control_flow = ControlFlow::Exit,
                Some(message) => problem_item.set_text(format!("⚠ {message}")),
            },
            tao::event::Event::UserEvent(UserEvent::Status(next)) => {
                if confirming.is_some_and(|(_, since)| since.elapsed().as_secs() >= CONFIRM_SECONDS)
                {
                    confirming = None;
                    uninstall_item.set_text(UNINSTALL_LABEL);
                    purge_item.set_text(PURGE_LABEL);
                }
                if next != state {
                    state = next;
                    let running = matches!(state, HostState::Running { .. });
                    let installed = !matches!(state, HostState::NotInstalled);
                    status_item.set_text(label(&state));
                    install_item.set_text(if !installed {
                        "Download host (acpd)…"
                    } else if running {
                        "Update host (stop it first)"
                    } else {
                        "Update host (download latest)…"
                    });
                    // Replacing the program while it runs is not possible on Windows.
                    install_item.set_enabled(!running);
                    toggle_item.set_text(if running { "Stop host" } else { "Start host" });
                    toggle_item.set_enabled(installed && !matches!(state, HostState::NotSetUp));
                    pair_item.set_enabled(running);
                    setup_item.set_enabled(installed);
                    setup_item.set_text(
                        if matches!(state, HostState::NotSetUp | HostState::NotInstalled) {
                            "Set up with Tailscale"
                        } else {
                            "Set up again with Tailscale"
                        },
                    );
                    if let Some(tray) = &tray {
                        let _ = tray.set_icon(Some(icon::icon(&state)));
                        let _ = tray.set_tooltip(Some(label(&state)));
                    }
                }
            }
            tao::event::Event::UserEvent(UserEvent::Tailscale(text)) => {
                tailscale_item.set_text(text)
            }
            tao::event::Event::UserEvent(UserEvent::Note(text)) => problem_item.set_text(text),
            tao::event::Event::UserEvent(UserEvent::Done(result)) => {
                problem_item.set_text(result.map(|m| format!("⚠ {m}")).unwrap_or_default());
            }
            tao::event::Event::UserEvent(UserEvent::Menu(id)) => {
                problem_item.set_text("");
                let asked = if id == uninstall_id {
                    Some(false)
                } else if id == purge_id {
                    Some(true)
                } else {
                    None
                };
                let confirmed = asked.is_some_and(|purge| {
                    confirming.is_some_and(|(was, since)| {
                        was == purge && since.elapsed().as_secs() < CONFIRM_SECONDS
                    })
                });
                uninstall_item.set_text(UNINSTALL_LABEL);
                purge_item.set_text(PURGE_LABEL);
                confirming = None;
                if let Some(purge) = asked {
                    if confirmed {
                        uninstall_item.set_enabled(false);
                        purge_item.set_enabled(false);
                        spawn_uninstall(&proxy, &control, purge);
                    } else {
                        confirming = Some((purge, std::time::Instant::now()));
                        let (item, text) = if purge {
                            (
                                &purge_item,
                                "Click again: stop, uninstall and DELETE settings",
                            )
                        } else {
                            (&uninstall_item, "Click again: stop the host and uninstall")
                        };
                        item.set_text(text);
                    }
                } else if id == quit_id {
                    *control_flow = ControlFlow::Exit;
                } else if id == install_id {
                    spawn_install(&proxy, &control);
                } else if id == setup_id {
                    {
                        let setup_proxy = proxy.clone();
                        spawn_action(&proxy, &control, move |cli| {
                            setup_with_tailscale(cli, &setup_proxy)
                        });
                    }
                } else if id == toggle_id {
                    if matches!(state, HostState::Running { .. }) {
                        spawn_action(&proxy, &control, |cli| cli.stop());
                    } else {
                        spawn_action(&proxy, &control, |cli| cli.start());
                    }
                } else if id == pair_id {
                    spawn_action(&proxy, &control, show_pairing);
                } else if id == folder_id {
                    let dir: &Path = &settings_dir();
                    let _ = std::fs::create_dir_all(dir);
                    let _ = open::that(dir);
                }
            }
            _ => {}
        }
    });
}
