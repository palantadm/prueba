#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, RunEvent, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

/// Set while the app is shutting down so closing windows doesn't forget them.
static QUITTING: AtomicBool = AtomicBool::new(false);
static LAST_SAVE: Mutex<Option<Instant>> = Mutex::new(None);

/// Geometry of one window, saved in physical pixels so it survives restarts.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct WinState {
    label: String,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    fullscreen: bool,
    maximized: bool,
}

struct Net {
    port: u16,
    lan: Option<String>,
    data_dir: std::path::PathBuf,
}

struct Server(Mutex<Option<lumos::RunningServer>>);

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "lumos=info".into()))
        .init();
    // The server lives on its own runtime for the whole process lifetime.
    let rt: &'static tokio::runtime::Runtime = Box::leak(Box::new(
        tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime"),
    ));

    let app = tauri::Builder::default()
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            let app = window.app_handle().clone();
            match event {
                WindowEvent::CloseRequested { .. } if label == "main" => {
                    // Closing the main window quits the app (and stops the server).
                    QUITTING.store(true, Ordering::SeqCst);
                    save_state(&app, true);
                    app.exit(0);
                }
                WindowEvent::Destroyed if label.starts_with("win-") && !QUITTING.load(Ordering::SeqCst) => {
                    save_state_without(&app, &label);
                }
                WindowEvent::Moved(_) | WindowEvent::Resized(_) => save_state(&app, false),
                _ => {}
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(move |app| {
            // Shows, media and library live in Documents/Lumos (easy to find and back up);
            // the webview keeps its own cache under the app data dir.
            let data_dir = match app.path().document_dir() {
                Ok(d) => d.join("Lumos"),
                Err(_) => app.path().app_data_dir()?.join("data"),
            };
            std::fs::create_dir_all(&data_dir)?;
            let server = rt.block_on(lumos::start(lumos::ServerConfig {
                bind: "0.0.0.0".into(),
                port: 8080,
                fallback_random: true,
                data_dir: data_dir.clone(),
            }))?;
            let net = Net { port: server.port, lan: server.lan_ip.clone(), data_dir };
            let url: tauri::Url = format!("http://127.0.0.1:{}/", net.port).parse()?;
            app.manage(net);
            app.manage(Server(Mutex::new(Some(server))));

            let saved = load_state(app.handle());
            let main_saved = saved.iter().find(|s| s.label == "main").cloned();
            let mut mb = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Lumos")
                .inner_size(1440.0, 900.0)
                .min_inner_size(1100.0, 680.0);
            if main_saved.is_none() {
                mb = mb.center();
            }
            let main = attach(mb, app.handle(), "main").build()?;
            if let Some(st) = &main_saved {
                apply_state(&main, st);
            }
            // Re-open the secondary windows that were open last time (their workspace is remembered by the page).
            for st in saved.iter().filter(|s| s.label.starts_with("win-")) {
                if let Some(id) = st.label.trim_start_matches("win-").parse::<u32>().ok() {
                    let _ = create_secondary(app.handle(), id, None, false, false, Some(st.clone()));
                }
            }

            app.set_menu(build_menu(app.handle())?)?;
            app.on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Lumos");

    app.run(move |handle, event| {
        if let RunEvent::ExitRequested { .. } = event {
            QUITTING.store(true, Ordering::SeqCst);
            save_state(handle, true);
        }
        if let RunEvent::Exit = event {
            if let Some(s) = handle.try_state::<Server>() {
                if let Some(server) = s.0.lock().unwrap().take() {
                    rt.block_on(server.stop());
                }
            }
        }
    });
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let file = Submenu::with_items(
        app,
        "Archivo",
        true,
        &[
            &MenuItem::with_id(app, "open_data", "Abrir carpeta de datos", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", "Salir", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edición",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Deshacer"))?,
            &PredefinedMenuItem::redo(app, Some("Rehacer"))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, Some("Cortar"))?,
            &PredefinedMenuItem::copy(app, Some("Copiar"))?,
            &PredefinedMenuItem::paste(app, Some("Pegar"))?,
            &PredefinedMenuItem::select_all(app, Some("Seleccionar todo"))?,
        ],
    )?;
    let view = Submenu::with_items(
        app,
        "Ver",
        true,
        &[
            &MenuItem::with_id(app, "fullscreen", "Pantalla completa", true, Some("F11"))?,
            &MenuItem::with_id(app, "reload", "Recargar", true, Some("CmdOrCtrl+R"))?,
            &MenuItem::with_id(app, "zoom_in", "Acercar", true, Some("CmdOrCtrl+="))?,
            &MenuItem::with_id(app, "zoom_out", "Alejar", true, Some("CmdOrCtrl+-"))?,
            &MenuItem::with_id(app, "zoom_reset", "Tamaño normal", true, Some("CmdOrCtrl+0"))?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Ventana",
        true,
        &[
            &MenuItem::with_id(app, "new_window", "Nueva ventana", true, Some("CmdOrCtrl+N"))?,
            &MenuItem::with_id(app, "move_next", "Mover al siguiente monitor", true, None::<&str>)?,
            &MenuItem::with_id(app, "fullscreen_here", "Pantalla completa en este monitor", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "close_window", "Cerrar ventana", true, Some("CmdOrCtrl+W"))?,
        ],
    )?;
    let help = Submenu::with_items(
        app,
        "Ayuda",
        true,
        &[
            &MenuItem::with_id(app, "help_manual", "Manual de ayuda", true, Some("F1"))?,
            &MenuItem::with_id(app, "help_commands", "Referencia de comandos", true, None::<&str>)?,
            &MenuItem::with_id(app, "help_keys", "Atajos de teclado", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "browser", "Abrir en el navegador", true, None::<&str>)?,
            &MenuItem::with_id(app, "net", "Dirección para tablet o móvil…", true, None::<&str>)?,
        ],
    )?;
    Menu::with_items(app, &[&file, &edit, &view, &window, &help])
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(net) = app.try_state::<Net>() else { return };
    let win = focused_window(app);
    match id {
        "quit" => {
            QUITTING.store(true, Ordering::SeqCst);
            save_state(app, true);
            app.exit(0)
        }
        "new_window" => {
            if let Some(w) = win {
                let _ = w.eval("window.dispatchEvent(new Event('lumos:new-window'))");
            }
        }
        "move_next" => {
            if let Some(w) = win {
                move_next_monitor(&w);
            }
        }
        "fullscreen_here" => {
            if let Some(w) = win {
                toggle_fullscreen(&w);
            }
        }
        "close_window" => {
            if let Some(w) = win {
                let _ = w.close();
            }
        }
        "help_manual" | "help_commands" | "help_keys" => {
            if let Some(w) = win {
                let page = match id {
                    "help_commands" => "commands",
                    "help_keys" => "shortcuts",
                    _ => "topic:inicio",
                };
                let _ = w.eval(format!("window.__lumosOpenHelp&&window.__lumosOpenHelp('{page}')"));
            }
        }
        "open_data" => {
            let _ = opener::open(&net.data_dir);
        }
        "browser" => {
            let _ = opener::open(format!("http://127.0.0.1:{}/", net.port));
        }
        "net" => {
            let msg = match &net.lan {
                Some(ip) => format!(
                    "Abre esta dirección en el navegador de una tablet o móvil conectado a la misma red:\n\nhttp://{ip}:{}\n\nSi no carga, permite Lumos en el cortafuegos del sistema.",
                    net.port
                ),
                None => "No se ha detectado una red local.".to_string(),
            };
            app.dialog().message(msg).title("Lumos en red").kind(MessageDialogKind::Info).show(|_| {});
        }
        "fullscreen" => {
            if let Some(w) = win {
                toggle_fullscreen(&w);
            }
        }
        "reload" => {
            if let Some(w) = win {
                let _ = w.eval("location.reload()");
            }
        }
        "zoom_in" => zoom(win, 0.1),
        "zoom_out" => zoom(win, -0.1),
        "zoom_reset" => {
            if let Some(w) = win {
                let _ = w.set_zoom(1.0);
            }
        }
        _ => {}
    }
}

fn zoom(win: Option<tauri::WebviewWindow>, delta: f64) {
    if let Some(w) = win {
        // Zoom is kept in the page so it survives menu actions without extra state.
        let _ = w.eval(&format!(
            "(()=>{{const z=Math.min(2,Math.max(0.5,(window.__z||1)+({delta})));window.__z=z;document.documentElement.style.zoom=z;}})()"
        ));
    }
}

// ───────────────────────────── multi-window ─────────────────────────────

fn focused_window(app: &AppHandle) -> Option<WebviewWindow> {
    let wins = app.webview_windows();
    wins.values().find(|w| w.is_focused().unwrap_or(false)).cloned().or_else(|| wins.get("main").cloned())
}

fn toggle_fullscreen(w: &WebviewWindow) {
    let fs = w.is_fullscreen().unwrap_or(false);
    let _ = w.set_fullscreen(!fs);
}

fn same_monitor(a: &Monitor, b: &Monitor) -> bool {
    a.position() == b.position() && a.size() == b.size()
}

fn next_monitor(w: &WebviewWindow) -> Option<Monitor> {
    let mons = w.available_monitors().ok()?;
    if mons.len() < 2 {
        return mons.into_iter().next();
    }
    let cur = w.current_monitor().ok().flatten();
    let idx = cur.and_then(|c| mons.iter().position(|m| same_monitor(m, &c))).unwrap_or(0);
    mons.get((idx + 1) % mons.len()).cloned()
}

/// Moves a window to the next monitor, keeping fullscreen / maximized state.
fn move_next_monitor(w: &WebviewWindow) {
    let Some(m) = next_monitor(w) else { return };
    let w = w.clone();
    std::thread::spawn(move || {
        let fs = w.is_fullscreen().unwrap_or(false);
        let max = w.is_maximized().unwrap_or(false);
        if fs {
            let _ = w.set_fullscreen(false);
        }
        if max {
            let _ = w.unmaximize();
        }
        std::thread::sleep(Duration::from_millis(120));
        let p = m.position();
        let _ = w.set_position(PhysicalPosition::new(p.x + 40, p.y + 40));
        std::thread::sleep(Duration::from_millis(160));
        if fs {
            let _ = w.set_fullscreen(true);
        } else if max {
            let _ = w.maximize();
        }
    });
}

/// A link to somewhere other than this app's own server (doc pages, key consoles…): hand it to the system browser.
fn open_external(url: &tauri::Url) -> bool {
    let web = matches!(url.scheme(), "http" | "https" | "mailto");
    let own = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]" | "::1"));
    if web && !own {
        let _ = opener::open(url.as_str());
        return true;
    }
    false
}

/// Adds the handlers every Lumos window needs: `window.open` → native window, and
/// `/__lumos/<cmd>` navigations (close / fullscreen / move) coming from the page.
fn attach<'a, M: Manager<tauri::Wry>>(b: WebviewWindowBuilder<'a, tauri::Wry, M>, app: &AppHandle, label: &str) -> WebviewWindowBuilder<'a, tauri::Wry, M> {
    let a1 = app.clone();
    let a2 = app.clone();
    let l1 = label.to_string();
    b.on_navigation(move |url| {
        if open_external(url) {
            return false;
        }
        let path = url.path();
        if let Some(cmd) = path.strip_prefix("/__lumos/") {
            let (app, label, cmd) = (a1.clone(), l1.clone(), cmd.to_string());
            std::thread::spawn(move || {
                let Some(w) = app.get_webview_window(&label) else { return };
                match cmd.as_str() {
                    "close" => {
                        let _ = w.close();
                    }
                    "fullscreen" => toggle_fullscreen(&w),
                    "move" => move_next_monitor(&w),
                    _ => {}
                }
            });
            return false;
        }
        true
    })
    .on_new_window(move |url, _features| {
        if open_external(&url) {
            return NewWindowResponse::Deny;
        }
        let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
        let app = a2.clone();
        let id = q.get("win").and_then(|v| v.parse::<u32>().ok()).unwrap_or_else(|| free_id(&app));
        let ws = q.get("ws").cloned();
        let mon = q.get("mon").map(|v| v == "next").unwrap_or(false);
        let fs = q.get("fs").map(|v| v == "1").unwrap_or(false);
        // Create the window off the event-handler thread.
        std::thread::spawn(move || {
            let _ = create_secondary(&app, id, ws.as_deref(), mon, fs, None);
        });
        NewWindowResponse::Deny
    })
}

fn free_id(app: &AppHandle) -> u32 {
    let used: Vec<u32> = app.webview_windows().keys().filter_map(|l| l.strip_prefix("win-").and_then(|n| n.parse().ok())).collect();
    (2..).find(|n| !used.contains(n)).unwrap_or(2)
}

/// Creates (or focuses) secondary window `win-<id>`.
fn create_secondary(app: &AppHandle, id: u32, ws: Option<&str>, monitor_next: bool, fullscreen: bool, restore: Option<WinState>) -> tauri::Result<()> {
    let label = format!("win-{id}");
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_focus();
        return Ok(());
    }
    let Some(net) = app.try_state::<Net>() else { return Ok(()) };
    let mut q = format!("win={id}&app=1");
    if let Some(ws) = ws {
        q.push_str(&format!("&ws={}", ws.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect::<String>()));
    }
    if fullscreen {
        q.push_str("&fs=1");
    }
    let url: tauri::Url = format!("http://127.0.0.1:{}/?{q}", net.port).parse().expect("url");
    let target = if monitor_next { focused_window(app).and_then(|w| next_monitor(&w)) } else { None };
    let b = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(url))
        .title(format!("Lumos — Ventana {id}"))
        .inner_size(1280.0, 800.0)
        .min_inner_size(900.0, 600.0);
    let win = attach(b, app, &label).build()?;
    if let Some(st) = &restore {
        apply_state(&win, st);
    } else if let Some(m) = target {
        let p = m.position();
        let _ = win.set_position(PhysicalPosition::new(p.x + 40, p.y + 40));
        if fullscreen {
            let w = win.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                let _ = w.set_fullscreen(true);
            });
        } else {
            let _ = win.maximize();
        }
    } else if fullscreen {
        let _ = win.set_fullscreen(true);
    } else {
        let _ = win.center();
    }
    save_state(app, true);
    Ok(())
}

// ───────────────────────────── remembered geometry ─────────────────────────────

fn state_file(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("windows.json"))
}

fn load_state(app: &AppHandle) -> Vec<WinState> {
    state_file(app).and_then(|f| std::fs::read(f).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn snapshot(app: &AppHandle, skip: Option<&str>) -> Vec<WinState> {
    app.webview_windows()
        .iter()
        .filter(|(l, _)| Some(l.as_str()) != skip)
        .filter_map(|(l, w)| {
            let p = w.outer_position().ok()?;
            let s = w.inner_size().ok()?;
            Some(WinState {
                label: l.clone(),
                x: p.x,
                y: p.y,
                w: s.width,
                h: s.height,
                fullscreen: w.is_fullscreen().unwrap_or(false),
                maximized: w.is_maximized().unwrap_or(false),
            })
        })
        .collect()
}

fn write_state(app: &AppHandle, list: &[WinState]) {
    if let (Some(f), Ok(json)) = (state_file(app), serde_json::to_vec_pretty(list)) {
        let _ = std::fs::write(f, json);
    }
}

/// Saves all window geometries. `force = false` throttles to one write per 600 ms.
fn save_state(app: &AppHandle, force: bool) {
    {
        let mut last = LAST_SAVE.lock().unwrap();
        if !force && last.map(|t| t.elapsed() < Duration::from_millis(600)).unwrap_or(false) {
            return;
        }
        *last = Some(Instant::now());
    }
    write_state(app, &snapshot(app, None));
}

fn save_state_without(app: &AppHandle, label: &str) {
    write_state(app, &snapshot(app, Some(label)));
}

/// Applies a saved geometry if its position is still on a connected monitor.
fn apply_state(w: &WebviewWindow, st: &WinState) {
    let mons = w.available_monitors().unwrap_or_default();
    let (cx, cy) = (st.x + st.w as i32 / 2, st.y + st.h as i32 / 2);
    let visible = mons.iter().any(|m| {
        let (p, s) = (m.position(), m.size());
        cx >= p.x && cx < p.x + s.width as i32 && cy >= p.y && cy < p.y + s.height as i32
    });
    if !visible {
        let _ = w.center();
        return;
    }
    let _ = w.set_size(PhysicalSize::new(st.w.max(600), st.h.max(400)));
    let _ = w.set_position(PhysicalPosition::new(st.x, st.y));
    if st.fullscreen {
        let w = w.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            let _ = w.set_fullscreen(true);
        });
    } else if st.maximized {
        let _ = w.maximize();
    }
}
