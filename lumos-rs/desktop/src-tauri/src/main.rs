#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
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

            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("Lumos")
                .inner_size(1440.0, 900.0)
                .min_inner_size(1100.0, 680.0)
                .center()
                .build()?;

            app.set_menu(build_menu(app.handle())?)?;
            app.on_menu_event(|app, ev| on_menu(app, ev.id().as_ref()));
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Lumos");

    app.run(move |handle, event| {
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
            &PredefinedMenuItem::quit(app, Some("Salir"))?,
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
    let help = Submenu::with_items(
        app,
        "Ayuda",
        true,
        &[
            &MenuItem::with_id(app, "browser", "Abrir en el navegador", true, None::<&str>)?,
            &MenuItem::with_id(app, "net", "Dirección para tablet o móvil…", true, None::<&str>)?,
        ],
    )?;
    Menu::with_items(app, &[&file, &edit, &view, &help])
}

fn on_menu(app: &AppHandle, id: &str) {
    let Some(net) = app.try_state::<Net>() else { return };
    let win = app.get_webview_window("main");
    match id {
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
                let fs = w.is_fullscreen().unwrap_or(false);
                let _ = w.set_fullscreen(!fs);
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
