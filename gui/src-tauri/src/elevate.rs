//! UAC 提权。策略与命令行版一致: 普通权限先直接尝试, 写入被拒时用
//! ShellExecuteExW("runas") 以管理员身份重新拉起自己, 附加 --elevated
//! 参数后台执行同一 install/restore, 结果写入临时 JSON, 主进程等待其
//! 退出后读取并回报前端。提权子进程用 MessageBox 展示结果。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::inject;
use crate::paths;

/// 提权子进程写回的操作结果
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ElevOutcome {
    pub ok: bool,
    pub message: String,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::WaitForSingleObject;
    use windows_sys::Win32::UI::Shell::{
        IsUserAnAdmin, ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, SW_SHOWNORMAL,
    };

    const ERROR_CANCELLED: u32 = 1223;

    pub fn is_admin() -> bool {
        unsafe { IsUserAnAdmin() != 0 }
    }

    /// 启动时自我提权: 当前非管理员时用 runas 以管理员身份重新拉起自身
    /// （保留原命令行参数）, 随后调用方应退出本进程。
    /// 返回 true 表示需要退出当前进程（提权重启成功 / 用户取消 / 请求失败）；
    /// 返回 false 表示无需提权（已是管理员）。
    pub fn relaunch_as_admin_if_needed() -> bool {
        if is_admin() {
            return false;
        }
        let exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return false,
        };
        // 收集原参数并做 Windows 命令行转义, 避免路径含空格/引号时丢失
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut cmdline = String::new();
        for a in &args {
            if a.contains(' ') || a.contains('"') {
                cmdline.push('"');
                cmdline.push_str(&a.replace('"', "\"\""));
                cmdline.push('"');
            } else {
                cmdline.push_str(a);
            }
            cmdline.push(' ');
        }

        let verb = wide("runas");
        let file = wide(&exe.to_string_lossy());
        let params = wide(&cmdline);
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_SHOWNORMAL;

        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            let msg = if err == ERROR_CANCELLED {
                "需要管理员权限才能修改 ZCode 安装目录, 程序将退出。".to_string()
            } else {
                format!("请求管理员权限失败 (错误码 {err}), 程序将退出。")
            };
            let caption = wide("ZCode 皮肤管理器");
            let text = wide(&msg);
            unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    text.as_ptr(),
                    caption.as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
            }
        }
        true
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// 以管理员身份拉起自身执行 install/restore, 阻塞等待其结束并读取结果。
    pub fn elevate_and_wait(
        command: &str,
        skin_id: Option<&str>,
        target: &Path,
    ) -> Result<ElevOutcome, String> {
        let exe = std::env::current_exe().map_err(|e| format!("无法定位当前程序: {e}"))?;
        let result_file = std::env::temp_dir().join(format!(
            "zcode-skin-manager-elev-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&result_file);
        let mut cmdline = format!(
            "--elevated --command {command} --target \"{}\"",
            target.display()
        );
        if let Some(id) = skin_id {
            cmdline.push_str(&format!(" --skin {id}"));
        }
        cmdline.push_str(&format!(" --result-file \"{}\"", result_file.display()));

        let verb = wide("runas");
        let file = wide(&exe.to_string_lossy());
        let params = wide(&cmdline);
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_SHOWNORMAL;

        let ok = unsafe { ShellExecuteExW(&mut sei) };
        if ok == 0 {
            let err = unsafe { GetLastError() };
            if err == ERROR_CANCELLED {
                return Ok(ElevOutcome {
                    ok: false,
                    message: "已取消管理员授权, 未做任何修改。".into(),
                });
            }
            return Err(format!("请求管理员权限失败 (错误码 {err})"));
        }
        if !sei.hProcess.is_null() {
            unsafe {
                // 循环等待: UAC 弹窗阶段可能耗时较久
                loop {
                    if WaitForSingleObject(sei.hProcess, 500) != WAIT_TIMEOUT {
                        break;
                    }
                }
                CloseHandle(sei.hProcess);
            }
        }
        let outcome = fs::read_to_string(&result_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(ElevOutcome {
                ok: false,
                message: "提权进程已结束, 但未返回结果 (可能被安全软件拦截).".into(),
            });
        let _ = fs::remove_file(&result_file);
        Ok(outcome)
    }

    /// 入口: 若本进程是 --elevated 提权子进程, 执行对应命令并退出。
    /// 返回 Some(退出码) 表示已处理, 主流程不再启动 GUI。
    pub fn maybe_run_elevated_cli() -> Option<i32> {
        let args: Vec<String> = std::env::args().collect();
        if !args.iter().any(|a| a == "--elevated") {
            return None;
        }
        let flag = |name: &str| {
            args.iter()
                .position(|a| a == name)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        let command = flag("--command").unwrap_or_default();
        let skin_id = flag("--skin");
        let target: PathBuf = flag("--target")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(paths::DEFAULT_TARGET));
        let result_file = flag("--result-file");

        let outcome = run(&command, skin_id.as_deref(), &target);
        if let Some(rf) = &result_file {
            let _ = fs::write(
                rf,
                serde_json::to_string(&outcome).unwrap_or_else(|_| {
                    format!("{{\"ok\":{},\"message\":\"结果序列化失败\"}}", outcome.ok)
                }),
            );
        }
        let caption = wide("ZCode 皮肤管理器 · 管理员操作");
        let text = wide(&outcome.message);
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                text.as_ptr(),
                caption.as_ptr(),
                if outcome.ok {
                    MB_OK | MB_ICONINFORMATION
                } else {
                    MB_OK | MB_ICONERROR
                },
            );
        }
        Some(if outcome.ok { 0 } else { 1 })
    }

    fn run(command: &str, skin_id: Option<&str>, target: &Path) -> ElevOutcome {
        match command {
            "install" => match skin_id.map(|id| inject::install_flow(target, id)) {
                Some(Ok(msg)) => ElevOutcome {
                    ok: true,
                    message: msg,
                },
                Some(Err(e)) => ElevOutcome {
                    ok: false,
                    message: e.to_string(),
                },
                None => ElevOutcome {
                    ok: false,
                    message: "缺少 --skin 参数".into(),
                },
            },
            "restore" => match inject::restore_flow(target) {
                Ok(msg) => ElevOutcome {
                    ok: true,
                    message: msg,
                },
                Err(e) => ElevOutcome {
                    ok: false,
                    message: e.to_string(),
                },
            },
            other => ElevOutcome {
                ok: false,
                message: format!("未知命令: {other}"),
            },
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn is_admin() -> bool {
        false
    }

    pub fn relaunch_as_admin_if_needed() -> bool {
        false
    }

    pub fn elevate_and_wait(
        _command: &str,
        _skin_id: Option<&str>,
        _target: &Path,
    ) -> Result<ElevOutcome, String> {
        Err("仅支持 Windows".into())
    }

    pub fn maybe_run_elevated_cli() -> Option<i32> {
        None
    }
}

pub use imp::{elevate_and_wait, is_admin, maybe_run_elevated_cli, relaunch_as_admin_if_needed};
