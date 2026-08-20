//! ZCode 皮肤管理器 GUI 后端。
//! 模块与 liquid_glass_skin.py 的功能块一一对应:
//! asar.rs(asar 读写) / inject.rs(注入还原状态) / skins.rs(皮肤扫描) /
//! elevate.rs(UAC 提权) / process.rs(进程检测) / paths.rs(路径与设置)。
//! tray.rs 为 GUI 附加的系统托盘（后台驻留）。

// 内部实现模块, pub 仅为支持 tests/ 集成测试, 不属于对外 API
#[doc(hidden)]
pub mod asar;
#[doc(hidden)]
pub mod elevate;
#[doc(hidden)]
pub mod inject;
#[doc(hidden)]
pub mod paths;
#[doc(hidden)]
pub mod process;
#[doc(hidden)]
pub mod skins;
mod tray;

use serde::Serialize;
use std::path::{Path, PathBuf};

/// install/restore 命令的返回: 提示消息 + 操作后的最新状态
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ActionOutcome {
    message: String,
    status: inject::StatusInfo,
}

fn target_from(explicit: Option<String>) -> PathBuf {
    paths::resolve_target(explicit.as_deref())
}

fn status_of(target: &Path) -> inject::StatusInfo {
    inject::status(target)
}

#[tauri::command]
async fn list_skins() -> Result<Vec<skins::SkinInfo>, String> {
    tauri::async_runtime::spawn_blocking(skins::list_infos)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_status(target: Option<String>) -> Result<inject::StatusInfo, String> {
    let t = target_from(target);
    tauri::async_runtime::spawn_blocking(move || inject::status(&t))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_settings() -> Result<paths::Settings, String> {
    tauri::async_runtime::spawn_blocking(paths::load_settings)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn save_settings(target_dir: String) -> Result<paths::Settings, String> {
    paths::save_settings(&target_dir)
}

#[tauri::command]
async fn detect_installs() -> Result<Vec<paths::DetectedInstall>, String> {
    tauri::async_runtime::spawn_blocking(paths::detect_install_dirs)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn install_skin(id: String, target: Option<String>) -> Result<ActionOutcome, String> {
    let t = target_from(target);
    let flow = {
        let t = t.clone();
        let id = id.clone();
        tauri::async_runtime::spawn_blocking(move || inject::install_flow(&t, &id))
            .await
            .map_err(|e| e.to_string())?
    };
    let message = match flow {
        Ok(msg) => msg,
        Err(inject::FlowError::Fatal(m)) => return Err(m),
        Err(inject::FlowError::NeedElevate(_)) => {
            // 目标目录需要管理员权限: 以 UAC 提权拉起自身执行同一操作
            let outcome = {
                let t = t.clone();
                let id = id.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    elevate::elevate_and_wait("install", Some(&id), &t)
                })
                .await
                .map_err(|e| e.to_string())??
            };
            if !outcome.ok {
                return Err(outcome.message);
            }
            outcome.message
        }
    };
    let status = tauri::async_runtime::spawn_blocking(move || status_of(&t))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionOutcome { message, status })
}

#[tauri::command]
async fn restore_skin(target: Option<String>) -> Result<ActionOutcome, String> {
    let t = target_from(target);
    let flow = {
        let t = t.clone();
        tauri::async_runtime::spawn_blocking(move || inject::restore_flow(&t))
            .await
            .map_err(|e| e.to_string())?
    };
    let message = match flow {
        Ok(msg) => msg,
        Err(inject::FlowError::Fatal(m)) => return Err(m),
        Err(inject::FlowError::NeedElevate(_)) => {
            let outcome = {
                let t = t.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    elevate::elevate_and_wait("restore", None, &t)
                })
                .await
                .map_err(|e| e.to_string())??
            };
            if !outcome.ok {
                return Err(outcome.message);
            }
            outcome.message
        }
    };
    let status = tauri::async_runtime::spawn_blocking(move || status_of(&t))
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionOutcome { message, status })
}

pub fn run() {
    // 提权子进程模式: 无 GUI, 只执行 install/restore 后退出
    if let Some(code) = elevate::maybe_run_elevated_cli() {
        std::process::exit(code);
    }
    tauri::Builder::default()
        .on_window_event(|window, event| {
            // 关闭主窗口 = 隐藏到托盘继续驻留, 真正退出走托盘菜单"退出"
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    window.hide().ok();
                    api.prevent_close();
                }
            }
        })
        .setup(|app| {
            tray::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_skins,
            get_status,
            get_settings,
            save_settings,
            detect_installs,
            install_skin,
            restore_skin
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
