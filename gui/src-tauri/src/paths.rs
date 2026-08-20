//! 路径解析: 安装目录设置 / 内置与用户皮肤目录 / ZCode 安装目录自动检测。
//! 设置持久化在 %APPDATA%\zcode-skin-manager\settings.json。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::process;

pub const DEFAULT_TARGET: &str = r"C:\Program Files\ZCode";

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub target_dir: String,
}

pub fn app_data_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let dir = base.join("zcode-skin-manager");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn user_skins_dir() -> PathBuf {
    app_data_dir().join("skins")
}

fn settings_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

pub fn load_settings() -> Settings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Settings {
            target_dir: DEFAULT_TARGET.to_string(),
        })
}

pub fn save_settings(target_dir: &str) -> Result<Settings, String> {
    let dir = target_dir.trim();
    if dir.is_empty() {
        return Err("安装目录不能为空".into());
    }
    let settings = Settings {
        target_dir: dir.to_string(),
    };
    let text = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), text).map_err(|e| format!("保存设置失败: {e}"))?;
    Ok(settings)
}

/// 解析目标安装目录: 显式参数 > 已保存设置 > 默认 Program Files
pub fn resolve_target(explicit: Option<&str>) -> PathBuf {
    if let Some(t) = explicit {
        if !t.trim().is_empty() {
            return PathBuf::from(t.trim());
        }
    }
    PathBuf::from(load_settings().target_dir)
}

fn dir_has_any_skin(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.path().join("skin.json").is_file())
        })
        .unwrap_or(false)
}

/// 内置皮肤目录: 开发时从 exe 向上逐级找仓库根的 skins/，
/// 打包后使用随应用分发的 exe 旁 skins/（tauri.conf.json resources）。
pub fn builtin_skins_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent().map(|p| p.to_path_buf());
    for _ in 0..8 {
        let Some(d) = dir else { break };
        let cand = d.join("skins");
        if dir_has_any_skin(&cand) {
            return Some(cand);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

// ---------- ZCode 安装目录自动检测 ----------

/// 自动检测到的一个 ZCode 安装目录
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DetectedInstall {
    pub path: String,
    /// 来源描述: 注册表 / 常见位置 / 运行中的进程
    pub source: String,
}

/// 目录是否为有效的 ZCode 安装位置(以 resources\app.asar 存在为准)
pub fn is_zcode_install(dir: &Path) -> bool {
    dir.join("resources").join("app.asar").is_file()
}

/// 自动检测本机 ZCode 安装目录, 三个来源依次收集并去重:
/// 注册表卸载信息 > 常见安装位置 > 运行中的进程。
/// 每个候选都以 resources\app.asar 存在为验证条件。
pub fn detect_install_dirs() -> Vec<DetectedInstall> {
    let mut found: Vec<DetectedInstall> = Vec::new();
    for dir in registry_scan::zcode_dirs() {
        push_candidate(&mut found, dir, "注册表");
    }
    for dir in common_install_dirs() {
        push_candidate(&mut found, dir, "常见位置");
    }
    for dir in running_process_dirs() {
        push_candidate(&mut found, dir, "运行中的进程");
    }
    found
}

fn push_candidate(out: &mut Vec<DetectedInstall>, dir: PathBuf, source: &str) {
    if !is_zcode_install(&dir) {
        return;
    }
    let key = process::normalize_str(&dir.to_string_lossy());
    if out
        .iter()
        .any(|d| process::normalize_str(&d.path) == key)
    {
        return;
    }
    out.push(DetectedInstall {
        path: dir.to_string_lossy().into_owned(),
        source: source.to_string(),
    });
}

/// 常见安装位置: Program Files / Program Files (x86) / 用户级 Programs 目录
fn common_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for (var, rel) in [
        ("ProgramFiles", "ZCode"),
        ("ProgramFiles(x86)", "ZCode"),
        ("LOCALAPPDATA", r"Programs\ZCode"),
        ("LOCALAPPDATA", "ZCode"),
    ] {
        if let Some(base) = std::env::var_os(var) {
            dirs.push(PathBuf::from(base).join(rel));
        }
    }
    dirs
}

/// 运行中的 ZCode 进程所在目录(与 process.rs 同款 PowerShell 查询)
fn running_process_dirs() -> Vec<PathBuf> {
    // 复用 process::powershell_zcode_paths, 已加 CREATE_NO_WINDOW 不弹黑窗
    let Ok(out) = process::powershell_zcode_paths().output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                None
            } else {
                Path::new(l).parent().map(Path::to_path_buf)
            }
        })
        .collect()
}

/// 从注册表卸载信息中找 ZCode 的安装目录
#[cfg(windows)]
mod registry_scan {
    use std::path::{Path, PathBuf};

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ,
    };

    // HKLM 两个视图(64 位 + WOW6432Node)与 HKCU 各有一份卸载信息
    const UNINSTALL_PATHS: [(HKEY, &str); 3] = [
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
        (
            HKEY_CURRENT_USER,
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        ),
    ];

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn zcode_dirs() -> Vec<PathBuf> {
        let mut out = Vec::new();
        for (root, sub) in UNINSTALL_PATHS {
            let Some(hroot) = open(root, sub) else { continue };
            for key in sub_keys(hroot) {
                let Some(hk) = open(hroot, &key) else { continue };
                if let Some(dir) = install_dir_of(hk) {
                    out.push(dir);
                }
                unsafe { RegCloseKey(hk) };
            }
            unsafe { RegCloseKey(hroot) };
        }
        out
    }

    fn open(root: HKEY, sub: &str) -> Option<HKEY> {
        let mut h: HKEY = std::ptr::null_mut();
        let ret = unsafe { RegOpenKeyExW(root, wide(sub).as_ptr(), 0, KEY_READ, &mut h) };
        (ret == ERROR_SUCCESS).then_some(h)
    }

    fn sub_keys(hroot: HKEY) -> Vec<String> {
        let mut names = Vec::new();
        let mut buf = [0u16; 256];
        for i in 0.. {
            let mut len = buf.len() as u32;
            let ret = unsafe {
                RegEnumKeyExW(
                    hroot,
                    i,
                    buf.as_mut_ptr(),
                    &mut len,
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            match ret {
                ERROR_SUCCESS => {
                    let end = buf.iter().position(|&u| u == 0).unwrap_or(len as usize);
                    names.push(String::from_utf16_lossy(&buf[..end]));
                }
                _ => break, // ERROR_NO_MORE_ITEMS 及其他错误都结束枚举
            }
        }
        names
    }

    /// 单个卸载条目 -> 安装目录: 先看 DisplayName 是否为 ZCode,
    /// InstallLocation 优先, 缺失时从 UninstallString / DisplayIcon 推导 exe 所在目录
    fn install_dir_of(hk: HKEY) -> Option<PathBuf> {
        let name = query_string(hk, "DisplayName")?;
        if !name.to_lowercase().contains("zcode") {
            return None;
        }
        if let Some(loc) = query_string(hk, "InstallLocation") {
            return Some(PathBuf::from(loc));
        }
        for value in ["UninstallString", "DisplayIcon"] {
            if let Some(dir) = query_string(hk, value).and_then(|s| dir_from_exe_str(&s)) {
                return Some(dir);
            }
        }
        None
    }

    fn query_string(hk: HKEY, name: &str) -> Option<String> {
        unsafe {
            let name_w = wide(name);
            let mut ty = REG_SZ;
            let mut size: u32 = 0;
            if RegQueryValueExW(
                hk,
                name_w.as_ptr(),
                std::ptr::null(),
                &mut ty,
                std::ptr::null_mut(),
                &mut size,
            ) != ERROR_SUCCESS
                || (ty != REG_SZ && ty != REG_EXPAND_SZ)
                || size < 2
            {
                return None;
            }
            let mut buf = vec![0u8; size as usize];
            if RegQueryValueExW(
                hk,
                name_w.as_ptr(),
                std::ptr::null(),
                std::ptr::null_mut(),
                buf.as_mut_ptr(),
                &mut size,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let units: Vec<u16> = buf[..size as usize]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .take_while(|&u| u != 0)
                .collect();
            let s = expand_env(String::from_utf16_lossy(&units).trim());
            (!s.is_empty()).then_some(s)
        }
    }

    /// 展开 REG_EXPAND_SZ 里未展开的 %VAR%(std 的 env::var 在 Windows 上大小写不敏感)
    fn expand_env(s: &str) -> String {
        if !s.contains('%') {
            return s.to_string();
        }
        let mut out = String::new();
        let mut rest = s;
        while let Some(start) = rest.find('%') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            match after.find('%') {
                Some(end) if end > 0 => {
                    match std::env::var(&after[..end]) {
                        Ok(v) => out.push_str(&v),
                        Err(_) => {
                            out.push('%');
                            out.push_str(&after[..end]);
                            out.push('%');
                        }
                    }
                    rest = &after[end + 1..];
                }
                _ => {
                    out.push('%');
                    out.push_str(after);
                    return out;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// 从 `"C:\...\Uninstall ZCode.exe" /S` 或 `C:\...\ZCode.exe,0`
    /// 这类字符串提取 exe 所在目录
    pub(crate) fn dir_from_exe_str(s: &str) -> Option<PathBuf> {
        let s = s.trim().trim_start_matches('"');
        let end = s.to_lowercase().find(".exe")? + 4;
        let exe = s.get(..end)?;
        Path::new(exe).parent().map(Path::to_path_buf)
    }
}

#[cfg(not(windows))]
mod registry_scan {
    use std::path::PathBuf;

    pub fn zcode_dirs() -> Vec<PathBuf> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_zcode_install_requires_asar() {
        let dir = crate::asar::temp_root("paths-asar-check");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("resources")).unwrap();
        assert!(!is_zcode_install(&dir));
        fs::write(dir.join("resources").join("app.asar"), b"x").unwrap();
        assert!(is_zcode_install(&dir));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_filters_and_dedupes() {
        let dir = crate::asar::temp_root("paths-detect");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("resources")).unwrap();
        fs::write(dir.join("resources").join("app.asar"), b"x").unwrap();

        let mut found: Vec<DetectedInstall> = Vec::new();
        // 无 asar 的目录被过滤
        push_candidate(&mut found, dir.parent().unwrap().to_path_buf(), "测试");
        assert!(found.is_empty());
        push_candidate(&mut found, dir.clone(), "测试");
        // 同一目录仅大小写不同只保留一条
        let upper = PathBuf::from(dir.to_string_lossy().to_uppercase());
        push_candidate(&mut found, upper, "测试2");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].source, "测试");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dir_from_exe_str_variants() {
        #[cfg(windows)]
        {
            use registry_scan::dir_from_exe_str;
            assert_eq!(
                dir_from_exe_str(r#""C:\Apps\ZCode\Uninstall ZCode.exe" /S"#),
                Some(PathBuf::from(r"C:\Apps\ZCode"))
            );
            assert_eq!(
                dir_from_exe_str(r"C:\Apps\ZCode\ZCode.exe,0"),
                Some(PathBuf::from(r"C:\Apps\ZCode"))
            );
            assert_eq!(dir_from_exe_str(r"C:\Apps\ZCode"), None);
        }
    }

    #[test]
    #[ignore = "依赖本机真实安装环境, 仅手动验证用"]
    fn detect_real_prints_installs() {
        for d in detect_install_dirs() {
            println!("[detect] {} <- {}", d.path, d.source);
        }
    }
}
