//! 系统托盘：常驻图标（合规要求：不可隐藏）、暂停/恢复、打开面板、退出。
//!
//! 退出流程：请求停止采集 → 等待写入器 flush（最多 ~1.5s）→ 退出进程。

use crate::state::AppState;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

/// 构建托盘图标与菜单（应用启动时调用一次）。
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let paused = app.state::<AppState>().shared.paused.load(Ordering::SeqCst);
    let menu = build_menu(app, paused)?;

    TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .expect("bundle 中必须包含应用图标")
                .clone(),
        )
        .tooltip(tooltip(paused))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            // 左键单击 → 打开主面板
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// 根据状态刷新托盘（暂停态文案与提示）。
pub fn refresh(app: &AppHandle) {
    let paused = app.state::<AppState>().shared.paused.load(Ordering::SeqCst);
    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(menu) = build_menu(app, paused) {
            let _ = tray.set_menu(Some(menu));
        }
        let _ = tray.set_tooltip(Some(tooltip(paused)));
    }
}

fn tooltip(paused: bool) -> &'static str {
    if paused {
        "TypeTrek — 已暂停采集"
    } else {
        "TypeTrek — 采集中"
    }
}

fn build_menu(app: &AppHandle, paused: bool) -> tauri::Result<Menu<Wry>> {
    let toggle_label = if paused {
        "恢复采集"
    } else {
        "暂停采集"
    };
    let toggle = MenuItem::with_id(app, "toggle_pause", toggle_label, true, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "打开主面板", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出 TypeTrek", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[&show, &toggle, &PredefinedMenuItem::separator(app)?, &quit],
    )
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "toggle_pause" => {
            let state = app.state::<AppState>();
            let mut cfg = state.config();
            cfg.capture_enabled = !cfg.capture_enabled;
            if let Err(e) = state.apply_config(cfg) {
                eprintln!("[typetrek] 暂停状态保存失败：{e}");
            } else {
                refresh(app);
            }
        }
        "show" => show_main_window(app),
        "quit" => quit_app(app),
        _ => {}
    }
}

/// 显示并聚焦主窗口（托盘菜单/单实例二次启动/前端调用）。
pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 退出：停止采集 → 等写入器 flush → 退出进程。
pub fn quit_app(app: &AppHandle) {
    let state = app.state::<AppState>();
    crate::capture::stop_capture(&state.shared);

    // 最多等待 1.5s 让写入器把队列里的事件落库（WAL 已提交部分不会丢）
    for _ in 0..30 {
        if !state.shared.writer_ready.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    app.exit(0);
}
