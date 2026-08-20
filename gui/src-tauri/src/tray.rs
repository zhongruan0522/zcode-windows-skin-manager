//! 系统托盘（后台驻留）。关闭主窗口不退出应用，仅隐藏到托盘；
//! 左键点击托盘图标唤起主窗口，右键菜单：回到主页面 / 前往 GitHub /
//! 建议反馈（浏览器打开 issue 页）/ 退出。

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

pub const TRAY_ID: &str = "main-tray";

mod menu_ids {
    pub const SHOW: &str = "show";
    pub const GITHUB: &str = "github";
    pub const FEEDBACK: &str = "feedback";
    pub const QUIT: &str = "quit";
}

/// 显示并聚焦主窗口（托盘左键 / 菜单"回到主页面"共用）
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("main") {
        if win.is_minimized().unwrap_or(false) {
            let _ = win.unminimize();
        }
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// 用系统默认浏览器打开链接。Windows 用 ShellExecuteW(null, "open", url)，
/// 与 elevate.rs 的提权拉起同属 Win32::UI::Shell。
fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }
        let verb = wide("open");
        let file = wide(url);
        unsafe {
            // 返回值 <= 32 表示失败（SE_ERR_*），静默处理：浏览器打不开时无更好出路
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
    }
}

/// 创建托盘图标。在 tauri::Builder 的 setup 回调中调用。
pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, menu_ids::SHOW, "回到主页面", true, None::<&str>)?;
    let github = MenuItem::with_id(app, menu_ids::GITHUB, "前往 GitHub", true, None::<&str>)?;
    let feedback = MenuItem::with_id(app, menu_ids::FEEDBACK, "建议反馈", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, menu_ids::QUIT, "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &github, &feedback, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("ZCode 皮肤管理器")
        .menu(&menu)
        // 左键点击不弹菜单（改为唤起主窗口），右键弹菜单
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            menu_ids::SHOW => show_main_window(app),
            menu_ids::GITHUB => open_url(
                "https://github.com/zhongruan0522/zcode-windows-skin-manager",
            ),
            menu_ids::FEEDBACK => open_url(
                "https://github.com/zhongruan0522/zcode-windows-skin-manager/issue",
            ),
            menu_ids::QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    // 复用 tauri.conf.json bundle.icon 嵌入的默认窗口图标（dev/release 均可用）
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}
