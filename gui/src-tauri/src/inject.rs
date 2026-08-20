//! install / restore / status 流程编排, 移植自 liquid_glass_skin.py 的
//! cmd_install / cmd_restore / cmd_status。注入方式: 在
//! out/renderer/index.html 的 </head> 前插入 <link>, 皮肤 CSS 写入
//! out/renderer/assets/liquid-glass.css, 并重算 asar integrity。

use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::asar;
use crate::process;
use crate::skins;

pub const HTML_RELPATH: &str = "out/renderer/index.html";
pub const CSS_RELPATH: &str = "out/renderer/assets/liquid-glass.css";
/// index.html 里的注入标记（保持与命令行版一致）
pub const MARKER: &str = "liquid-glass-skin";
const BACKUP_NAME: &str = "app.asar.orig";
const TMP_NAME: &str = "app.asar.lgtmp";

/// 注入的 CSS 首行会携带皮肤 id, status 据此识别当前安装的皮肤
const SKIN_ID_LINE_PREFIX: &str = "/* zcode-skin-id:";

#[derive(Debug)]
pub enum FlowError {
    /// 当前权限不足, 需要 UAC 提权重试
    NeedElevate(String),
    /// 无法继续的错误
    Fatal(String),
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::NeedElevate(m) => write!(f, "需要管理员权限: {m}"),
            FlowError::Fatal(m) => write!(f, "{m}"),
        }
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    pub target_dir: String,
    pub asar_exists: bool,
    pub has_backup: bool,
    pub installed_skin_id: Option<String>,
    pub installed_skin_name: Option<String>,
    pub zcode_running: bool,
    pub is_elevated: bool,
}

pub fn asar_path(target: &Path) -> PathBuf {
    target.join("resources").join("app.asar")
}

pub fn backup_path(target: &Path) -> PathBuf {
    let mut p = asar_path(target);
    p.set_file_name(BACKUP_NAME);
    p
}

fn tmp_path(target: &Path) -> PathBuf {
    let mut p = asar_path(target);
    p.set_file_name(TMP_NAME);
    p
}

fn is_denied(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(5)
}

/// 在 </head> 前插入皮肤 <link>; 已注入过(含标记)则原样返回, 幂等
pub fn inject_link(html: &str) -> Result<String, String> {
    if html.contains(MARKER) {
        return Ok(html.to_string());
    }
    let link = format!(
        "<!--{MARKER}--><link rel=\"stylesheet\" crossorigin href=\"./assets/liquid-glass.css\">"
    );
    match html.find("</head>") {
        Some(pos) => Ok(format!("{}{}{}", &html[..pos], link, &html[pos..])),
        None => Err("index.html 中找不到 </head>".into()),
    }
}

/// 从注入 CSS 首行解析皮肤 id（形如 `/* zcode-skin-id: liquid-glass */`）
pub fn parse_skin_id(css: &str) -> Option<&str> {
    let first = css.lines().next()?;
    let rest = first
        .trim()
        .strip_prefix(SKIN_ID_LINE_PREFIX)?
        .trim_end_matches("*/")
        .trim();
    if rest.is_empty() || rest.contains(char::is_whitespace) {
        None
    } else {
        Some(rest)
    }
}

fn read_file_from_asar(path: &Path, relpath: &str) -> Result<Vec<u8>, String> {
    let mut f = fs::File::open(path).map_err(|e| format!("打开 {} 失败: {e}", path.display()))?;
    let (header, data_start) = asar::parse_header(&mut f)?;
    asar::read_inner(&mut f, &header, data_start, relpath)
        .map_err(|e| format!("读取 {relpath} 失败: {e}"))?
        .ok_or_else(|| format!("asar 中找不到 {relpath}"))
}

/// 应用皮肤: 备份 -> 以备份为底稿重建 asar -> 写回。幂等, 可直接换肤。
pub fn install_flow(target: &Path, skin_id: &str) -> Result<String, FlowError> {
    let asar = asar_path(target);
    if !asar.is_file() {
        return Err(FlowError::Fatal(format!(
            "找不到 {}\n请在「设置」中确认 ZCode 安装目录",
            asar.display()
        )));
    }
    if process::zcode_running_under(target) {
        return Err(FlowError::Fatal(
            "目标目录下的 ZCode.exe 正在运行, 请先完全退出 ZCode 桌面版再应用皮肤。".into(),
        ));
    }

    let skin = skins::find(skin_id).map_err(FlowError::Fatal)?;

    // 备份官方包(仅首次); 之后始终以备份为底稿重建, 天然幂等
    let backup = backup_path(target);
    if !backup.is_file() {
        match fs::copy(&asar, &backup) {
            Ok(_) => {}
            Err(e) if is_denied(&e) => {
                return Err(FlowError::NeedElevate(
                    "备份官方 app.asar 需要管理员权限".into(),
                ))
            }
            Err(e) => return Err(FlowError::Fatal(format!("备份失败: {e}"))),
        }
    }

    let html_bytes = read_file_from_asar(&backup, HTML_RELPATH).map_err(FlowError::Fatal)?;
    let html_text = String::from_utf8(html_bytes)
        .map_err(|_| FlowError::Fatal("index.html 不是有效的 UTF-8".into()))?;
    let new_html = inject_link(&html_text).map_err(FlowError::Fatal)?;
    let css = format!("{} {} */\n{}", SKIN_ID_LINE_PREFIX, skin_id, skin.css);

    let tmp = tmp_path(target);
    if let Err(m) = asar::build_patched_asar(
        &backup,
        &tmp,
        vec![(HTML_RELPATH.into(), new_html.into_bytes())],
        vec![(CSS_RELPATH.into(), css.into_bytes())],
    ) {
        let _ = fs::remove_file(&tmp);
        return Err(FlowError::Fatal(format!("重建 asar 失败: {m}")));
    }
    match fs::rename(&tmp, &asar) {
        Ok(_) => {}
        Err(e) if is_denied(&e) => {
            let _ = fs::remove_file(&tmp);
            return Err(FlowError::NeedElevate(
                "写回 app.asar 需要管理员权限".into(),
            ));
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(FlowError::Fatal(format!("写回 app.asar 失败: {e}")));
        }
    }
    Ok(format!("已应用皮肤「{}」。", skin.info.name))
}

/// 还原官方 app.asar（用备份覆盖, 备份随之移除）
pub fn restore_flow(target: &Path) -> Result<String, FlowError> {
    let asar = asar_path(target);
    let backup = backup_path(target);
    if !backup.is_file() {
        return Err(FlowError::Fatal(format!(
            "找不到备份 {}, 无需还原或备份已丢失。",
            backup.display()
        )));
    }
    if process::zcode_running_under(target) {
        return Err(FlowError::Fatal(
            "目标目录下的 ZCode.exe 正在运行, 请先完全退出 ZCode 桌面版再还原。".into(),
        ));
    }
    match fs::rename(&backup, &asar) {
        Ok(_) => Ok("已还原官方皮肤。".into()),
        Err(e) if is_denied(&e) => Err(FlowError::NeedElevate(
            "还原 app.asar 需要管理员权限".into(),
        )),
        Err(e) => Err(FlowError::Fatal(format!("还原失败: {e}"))),
    }
}

/// 查询注入状态。asar 损坏等异常按"未安装"处理, 不报错。
///
/// 注意: 这里**不再**调 `process::zcode_running_under`。
/// 启动 / 刷新状态时若拉 PowerShell 会闪一个黑色控制台窗口, 影响体验;
/// 而应用 / 还原流程内部(`install_flow` / `restore_flow`)和前端守卫
/// (`api.zcodeRunning`) 已经各自做过运行检测, 此处无需再查。
/// 因此 `zcode_running` 字段恒为 `false`, StatusBar 的"运行中"提示
/// 改由前端按需调用 `zcode_running` 命令刷新。
pub fn status(target: &Path) -> StatusInfo {
    let asar = asar_path(target);
    let mut info = StatusInfo {
        target_dir: target.display().to_string(),
        asar_exists: asar.is_file(),
        has_backup: backup_path(target).is_file(),
        installed_skin_id: None,
        installed_skin_name: None,
        zcode_running: false,
        is_elevated: crate::elevate::is_admin(),
    };
    if !info.asar_exists {
        return info;
    }
    let Ok(mut f) = fs::File::open(&asar) else {
        return info;
    };
    let Ok((header, data_start)) = asar::parse_header(&mut f) else {
        return info;
    };
    let html = asar::read_inner(&mut f, &header, data_start, HTML_RELPATH)
        .ok()
        .flatten();
    let installed = html
        .map(|h| String::from_utf8_lossy(&h).contains(MARKER))
        .unwrap_or(false);
    if installed {
        let css_text = asar::read_inner(&mut f, &header, data_start, CSS_RELPATH)
            .ok()
            .flatten()
            .map(|c| String::from_utf8_lossy(&c).into_owned());
        // 优先读注入时写入的 id 标记; 命令行版注入的旧包按 css 内容识别
        let id = css_text
            .as_deref()
            .and_then(parse_skin_id)
            .map(str::to_string)
            .or_else(|| css_text.as_deref().and_then(legacy_match_skin_id));
        let name = id
            .as_deref()
            .and_then(|i| skins::find(i).ok().map(|s| s.info.name))
            .unwrap_or_else(|| "未知皮肤".to_string());
        info.installed_skin_id = id;
        info.installed_skin_name = Some(name);
    }
    info
}

/// 命令行版注入的 css 没有 id 标记, 与内置皮肤 css 逐字对比识别
fn legacy_match_skin_id(css: &str) -> Option<String> {
    skins::scan_all()
        .into_iter()
        .find(|pkg| css == pkg.css)
        .map(|pkg| pkg.info.id)
}

// ============================================================
// 流程测试（针对合成 asar, 不触碰真实安装目录）
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::asar::{build_test_asar, temp_root};

    const HTML: &[u8] = b"<!doctype html><html><head><title>t</title></head><body></body></html>";

    fn make_target(dir: &Path) {
        fs::create_dir_all(dir.join("resources")).unwrap();
        fs::write(
            dir.join("resources").join("app.asar"),
            build_test_asar(&[("out/renderer/index.html", HTML), ("other.txt", b"data")]),
        )
        .unwrap();
    }

    #[test]
    fn inject_link_before_head_close() {
        let out = inject_link("<html><head></head></html>").unwrap();
        assert!(out.contains(MARKER));
        assert!(out.contains("</head>"));
        assert!(out.find(MARKER).unwrap() < out.find("</head>").unwrap());
        // 幂等
        let again = inject_link(&out).unwrap();
        assert_eq!(again, out);
    }

    #[test]
    fn inject_link_without_head_fails() {
        assert!(inject_link("<html><body></body></html>").is_err());
    }

    #[test]
    fn parse_skin_id_roundtrip() {
        assert_eq!(
            parse_skin_id("/* zcode-skin-id: liquid-glass */\n:root{}"),
            Some("liquid-glass")
        );
        assert_eq!(parse_skin_id(":root{}"), None);
        assert_eq!(parse_skin_id("/* zcode-skin-id: a b */"), None);
    }

    #[test]
    fn install_status_swap_restore_full_cycle() {
        let dir = temp_root("inject-flow");
        make_target(&dir);

        // 未安装时: 无标记, 官方原版状态
        let st = status(&dir);
        assert!(st.asar_exists && !st.has_backup && st.installed_skin_id.is_none());

        // 应用 liquid-glass
        install_flow(&dir, "liquid-glass").unwrap();
        let st = status(&dir);
        assert_eq!(st.installed_skin_id.as_deref(), Some("liquid-glass"));
        assert_eq!(st.installed_skin_name.as_deref(), Some("液态玻璃"));
        assert!(st.has_backup);

        let html = String::from_utf8(read_file_from_asar(&asar_path(&dir), HTML_RELPATH).unwrap())
            .unwrap();
        assert_eq!(html.matches(MARKER).count(), 1, "link 只能注入一次");
        let css = read_file_from_asar(&asar_path(&dir), CSS_RELPATH).unwrap();
        assert!(css.starts_with(b"/* zcode-skin-id: liquid-glass */"));

        // 幂等: 重复应用同一皮肤, link 仍只有一条
        install_flow(&dir, "liquid-glass").unwrap();
        let html2 = String::from_utf8(read_file_from_asar(&asar_path(&dir), HTML_RELPATH).unwrap())
            .unwrap();
        assert_eq!(html2.matches(MARKER).count(), 1);

        // 直接换肤(不先还原)
        install_flow(&dir, "transparent-test").unwrap();
        let st = status(&dir);
        assert_eq!(st.installed_skin_id.as_deref(), Some("transparent-test"));

        // 还原: 回到官方 html, 备份被移除
        restore_flow(&dir).unwrap();
        let st = status(&dir);
        assert!(st.installed_skin_id.is_none() && !st.has_backup);
        let html3 = String::from_utf8(read_file_from_asar(&asar_path(&dir), HTML_RELPATH).unwrap())
            .unwrap();
        assert_eq!(html3.as_bytes(), HTML);
        // css 条目随重建消失
        assert!(read_file_from_asar(&asar_path(&dir), CSS_RELPATH).is_err());

        // 再次还原: 无备份, 报错
        assert!(matches!(restore_flow(&dir), Err(FlowError::Fatal(_))));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_missing_target_fails() {
        let dir = temp_root("inject-missing");
        assert!(matches!(
            install_flow(&dir, "liquid-glass"),
            Err(FlowError::Fatal(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn install_unknown_skin_fails() {
        let dir = temp_root("inject-unknown");
        make_target(&dir);
        assert!(matches!(
            install_flow(&dir, "no-such-skin"),
            Err(FlowError::Fatal(_))
        ));
        let _ = fs::remove_dir_all(&dir);
    }
}
