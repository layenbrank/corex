//! Tauri 完整 wiring 示例：sidecar + 托盘 + 全局快捷键 + corex IPC
//!
//! 复制到 Tauri 项目：`src-tauri/src/lib.rs`（或合并进现有 lib.rs）
//!
//! 配套文件：
//! - `corex_ipc.rs`          → src-tauri/src/corex_ipc.rs
//! - `tauri.conf.json`       → src-tauri/tauri.conf.json（合并 bundle 段）
//! - `capabilities/default.json` → src-tauri/capabilities/default.json（合并 permissions）
//! - `scripts/copy-corex-serve.mjs` → 项目根 scripts/

mod corex_ipc;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent, State,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

/// 截图保存目录（可改为从配置 / store 读取）
const SCREENSHOT_DIR: &str = "C:/Screenshots";

struct AppState {
    screenshot_dir: Mutex<PathBuf>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(on_global_shortcut)
                .build(),
        )
        .manage(AppState {
            screenshot_dir: Mutex::new(PathBuf::from(SCREENSHOT_DIR)),
        })
        .invoke_handler(tauri::generate_handler![
            take_screenshot,
            get_screenshot_dir,
            set_screenshot_dir,
        ])
        .setup(setup_app)
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(on_run_event);
}

fn setup_app(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    spawn_corex_sidecar(app.handle())?;
    build_tray(app)?;
    register_hotkeys(app.handle())?;
    Ok(())
}

/// 通过 Tauri sidecar 启动 corex-serve
fn spawn_corex_sidecar(app: &AppHandle) -> Result<(), String> {
    let sidecar = app
        .shell()
        .sidecar("binaries/corex-serve")
        .map_err(|e| format!("创建 sidecar 命令失败: {e}"))?
        .args(["--pipe", corex_ipc::PIPE_NAME]);

    let (mut rx, _child) = sidecar
        .spawn()
        .map_err(|e| format!("启动 corex-serve 失败: {e}"))?;

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let CommandEvent::Error(err) = event {
                eprintln!("[corex-serve] {err}");
            }
        }
        let _ = app_handle;
    });

    // 等待 Pipe 就绪（首次启动 xcap 初始化可能较慢）
    wait_for_daemon(Duration::from_secs(8));
    Ok(())
}

fn wait_for_daemon(timeout: Duration) {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if corex_ipc::is_ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    eprintln!("[corex] 警告: Daemon 在超时内未就绪，快捷键可能失败");
}

fn build_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let screenshot = MenuItem::with_id(app, "screenshot", "截图", true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&screenshot, &show, &quit])?;

    let icon = app
        .default_window_icon()
        .ok_or("缺少应用图标")?
        .clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Corex 截图")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "screenshot" => trigger_screenshot(app.clone()),
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                trigger_screenshot(tray.app_handle().clone());
            }
        })
        .build(app)?;

    Ok(())
}

fn register_hotkeys(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyS);
    app.global_shortcut().register(shortcut)?;
    Ok(())
}

fn on_global_shortcut(
    app: &AppHandle,
    _shortcut: &Shortcut,
    event: tauri_plugin_global_shortcut::ShortcutEvent,
) {
    if event.state() == ShortcutState::Pressed {
        trigger_screenshot(app.clone());
    }
}

fn trigger_screenshot(app: AppHandle) {
    let dir = app
        .state::<AppState>()
        .screenshot_dir
        .lock()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| SCREENSHOT_DIR.to_string());

    std::thread::spawn(move || match corex_ipc::screenshot(&dir) {
        Ok(path) => {
            let _ = app.emit("screenshot-done", path);
        }
        Err(err) => {
            eprintln!("[screenshot] {err}");
            let _ = app.emit("screenshot-error", err);
        }
    });
}

fn on_run_event(_app: &AppHandle, event: RunEvent) {
    if matches!(event, RunEvent::Exit) {
        let _ = corex_ipc::shutdown();
    }
}

// ── Tauri Commands（前端也可调用）────────────────────────────────────────

#[tauri::command]
fn take_screenshot(state: State<AppState>) -> Result<String, String> {
    let dir = state
        .screenshot_dir
        .lock()
        .map_err(|_| "screenshot_dir lock poisoned".to_string())?
        .to_string_lossy()
        .into_owned();
    corex_ipc::screenshot(dir)
}

#[tauri::command]
fn get_screenshot_dir(state: State<AppState>) -> Result<String, String> {
    Ok(state
        .screenshot_dir
        .lock()
        .map_err(|_| "screenshot_dir lock poisoned".to_string())?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
fn set_screenshot_dir(state: State<'_, AppState>, dir: String) -> Result<(), String> {
    *state
        .screenshot_dir
        .lock()
        .map_err(|_| "screenshot_dir lock poisoned".to_string())? = PathBuf::from(dir);
    Ok(())
}
