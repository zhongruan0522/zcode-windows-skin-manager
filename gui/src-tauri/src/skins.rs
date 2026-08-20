//! 皮肤包扫描与校验。规范见根目录 AGENTS.md:
//! skins/<skin-id>/{skin.json(必需), skin.css(必需), preview.png(推荐), assets/(可选)}。
//! 内置目录 + 用户目录(~/.zcode-skins/skins)合并, 同名用户优先。

use base64::Engine;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::paths;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SkinInfo {
    pub id: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    /// builtin | user
    pub source: String,
    pub preview_data_url: Option<String>,
}

pub struct SkinPackage {
    pub info: SkinInfo,
    /// 皮肤包目录（assets 等资源的注入来源, 预留）
    #[allow(dead_code)]
    pub dir: PathBuf,
    pub css: String,
}

fn scan_dir(base: &Path, source: &str, out: &mut Vec<SkinPackage>) {
    let Ok(rd) = fs::read_dir(base) else { return };
    for entry in rd.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(id) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(json_text) = fs::read_to_string(dir.join("skin.json")) else {
            continue;
        };
        let Ok(meta) = serde_json::from_str::<serde_json::Value>(&json_text) else {
            continue;
        };
        let Some(name) = meta.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if name.trim().is_empty() {
            continue;
        }
        let Ok(css) = fs::read_to_string(dir.join("skin.css")) else {
            continue;
        };
        let info = SkinInfo {
            id: id.to_string(),
            name: name.trim().to_string(),
            author: meta
                .get("author")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            version: meta
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            description: meta
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            source: source.to_string(),
            preview_data_url: read_preview(&dir, meta.get("preview").and_then(|v| v.as_str())),
        };
        out.push(SkinPackage { info, dir, css });
    }
}

/// 列出内置皮肤目录下的所有 id(仅用于 source 标记, 不读取内容)
fn builtin_ids() -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    let Some(b) = paths::builtin_skins_dir() else {
        return out;
    };
    let Ok(rd) = fs::read_dir(&b) else {
        return out;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join("skin.json").is_file() {
            if let Some(name) = entry.file_name().to_str() {
                out.insert(name.to_string());
            }
        }
    }
    out
}

/// 删除用户皮肤目录下的指定皮肤; 不允许删除内置皮肤源目录(本函数只动用户目录)。
/// 返回被删除皮肤的友好名称(供前端展示)。
pub fn delete(id: &str) -> Result<String, String> {
    if !valid_skin_id(id) {
        return Err("皮肤 id 非法".into());
    }
    let dir = paths::user_skins_dir().join(id);
    if !dir.is_dir() {
        return Err(format!("皮肤「{id}」不存在, 可能已被删除"));
    }
    // 取名称用于返回, 取不到就用 id
    let name = fs::read_to_string(dir.join("skin.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_else(|| id.to_string());
    fs::remove_dir_all(&dir).map_err(|e| format!("删除皮肤「{name}」失败: {e}"))?;
    Ok(name)
}

fn valid_skin_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id
            .chars()
            .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

fn read_preview(dir: &Path, declared: Option<&str>) -> Option<String> {
    let mut cands: Vec<PathBuf> = Vec::new();
    if let Some(d) = declared {
        cands.push(dir.join(d));
    }
    for ext in ["png", "jpg", "jpeg", "webp", "gif"] {
        cands.push(dir.join(format!("preview.{ext}")));
    }
    for c in cands {
        let Ok(bytes) = fs::read(&c) else { continue };
        let mime = match c
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => continue,
        };
        return Some(format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    None
}

/// 扫描用户皮肤目录(~/.zcode-skins/skins, 内置皮肤已种子化到此);
/// source 字段按 id 是否出现在内置皮肤目录中标记 builtin / user。
pub fn scan_all() -> Vec<SkinPackage> {
    // 先确保内置皮肤已种子化(首次启动时一次性铺开, 之后早退)
    paths::ensure_builtin_skins_copied();
    let builtin = builtin_ids();
    let mut list: Vec<SkinPackage> = Vec::new();
    scan_dir(&paths::user_skins_dir(), "user", &mut list);
    // 根据 id 是否在内置目录中改写 source
    for pkg in list.iter_mut() {
        if builtin.contains(&pkg.info.id) {
            pkg.info.source = "builtin".to_string();
        }
    }
    // 按 id 排序, 保证列表顺序稳定
    let mut by_id: BTreeMap<String, SkinPackage> = BTreeMap::new();
    for pkg in list {
        by_id.insert(pkg.info.id.clone(), pkg);
    }
    by_id.into_values().collect()
}

pub fn list_infos() -> Vec<SkinInfo> {
    scan_all().into_iter().map(|p| p.info).collect()
}

pub fn find(id: &str) -> Result<SkinPackage, String> {
    scan_all()
        .into_iter()
        .find(|p| p.info.id == id)
        .ok_or_else(|| format!("找不到皮肤包「{id}」, 可能已被移动或删除"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_skins_loadable() {
        let pkgs = scan_all();
        // 仓库自带两个内置皮肤(测试二进制位于 target/debug/deps, 向上可找到仓库根 skins/)
        assert!(
            pkgs.iter().any(|p| p.info.id == "liquid-glass"),
            "应包含 liquid-glass, 实际: {:?}",
            pkgs.iter().map(|p| &p.info.id).collect::<Vec<_>>()
        );
        let lg = find("liquid-glass").unwrap();
        assert_eq!(lg.info.name, "液态玻璃");
        assert!(lg.css.contains("--color-panel"));
        assert!(lg.info.preview_data_url.is_some());
        assert!(lg.dir.join("skin.css").is_file());
    }
}
