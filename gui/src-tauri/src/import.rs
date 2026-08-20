//! ZIP 皮肤包导入。支持两种压缩包布局(整包一致, 不可混用):
//!   skins/<id>/{skin.json, skin.css, preview.png?, assets/?}
//!   <外壳目录>/skins/<id>/{...}
//! 校验通过后解压进用户皮肤目录(~/.zcode-skins/skins, 不存在则自动创建),
//! 同名皮肤整体覆盖; 任一皮肤校验失败则整包拒绝, 不留半成品。

use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::paths;

/// 导入成功的一个皮肤
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportedSkin {
    pub id: String,
    pub name: String,
}

/// import 命令的返回: 导入的皮肤列表 + 汇总消息
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub skins: Vec<ImportedSkin>,
    pub message: String,
}

/// 一个待导入的皮肤: zip 条目索引 + 条目在皮肤目录内的相对路径
#[derive(Default)]
struct PendingSkin {
    files: Vec<(usize, Vec<String>)>,
}

/// zip 内一个文件条目: (条目索引, 逐段安全路径)
type RawEntry = (usize, Vec<String>);

/// 导入 zip 到用户皮肤目录(~/.zcode-skins/skins, 自动创建)
pub fn import_zip(zip_path: &Path) -> Result<ImportOutcome, String> {
    import_zip_into(zip_path, &paths::user_skins_dir())
}

/// 导入 zip 到指定皮肤根目录(测试用, 不触碰真实用户数据)
pub fn import_zip_into(zip_path: &Path, skins_root: &Path) -> Result<ImportOutcome, String> {
    if !zip_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        return Err("仅支持导入 .zip 格式的压缩包".into());
    }
    let file = File::open(zip_path)
        .map_err(|e| format!("无法打开压缩包「{}」: {e}", zip_path.display()))?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|e| format!("「{}」不是有效的 ZIP 压缩包: {e}", zip_path.display()))?;

    // 第一轮: 解析结构, 确定皮肤集合(外壳目录必须全包唯一, 仅用于校验)
    let entries = collect_entries(&mut archive)?;
    let skins = classify_entries(entries)?;
    let valid: Vec<(&String, &PendingSkin)> = skins
        .iter()
        .filter(|(id, s)| valid_skin_id(id) && has_root_file(s, "skin.json") && has_root_file(s, "skin.css"))
        .collect();
    if valid.is_empty() {
        return Err(
            "压缩包结构不符合要求: 未找到有效的皮肤目录。\n\
             期望压缩包内包含 skins/<皮肤目录>/{skin.json, skin.css},\n\
             或 <名称>/skins/<皮肤目录>/{skin.json, skin.css}"
                .into(),
        );
    }

    // 第二轮: 先解压到暂存目录, 全部校验通过后一次性提交
    fs::create_dir_all(skins_root).map_err(|e| format!("创建皮肤目录失败: {e}"))?;
    let staging = skins_root.join(format!(
        ".importing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    let _ = fs::remove_dir_all(&staging);
    let result = extract_and_commit(&mut archive, &valid, &staging, skins_root);
    let _ = fs::remove_dir_all(&staging);
    result
}

/// 读取全部文件条目; 含 ..、绝对路径等危险成分或为目录的条目直接丢弃
fn collect_entries<R: io::Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Result<Vec<RawEntry>, String> {
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let entry = archive
            .by_index(i)
            .map_err(|e| format!("读取压缩包条目失败: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if !parts.is_empty() {
            out.push((i, parts));
        }
    }
    Ok(out)
}

/// 归类全部条目到 [外壳?]/skins/<id>/<文件...>; 外壳超过一层或与皮肤无关的条目忽略
fn classify_entries(entries: Vec<RawEntry>) -> Result<BTreeMap<String, PendingSkin>, String> {
    let mut skins: BTreeMap<String, PendingSkin> = BTreeMap::new();
    let mut shell_seen: Option<Option<String>> = None;
    for (index, parts) in entries {
        let Some((shell, id, file)) = classify(&parts) else {
            continue;
        };
        // 跳过系统垃圾文件(mac 压缩产物 / 缩略图缓存等)
        if is_junk_name(parts.last().map(|s| s.as_str()).unwrap_or_default()) {
            continue;
        }
        match (&shell_seen, shell) {
            (None, s) => shell_seen = Some(s),
            (Some(a), ref b) if a == b => {}
            _ => return Err("压缩包内存在多个不同的 skins 目录, 无法确定要导入的内容".into()),
        }
        skins.entry(id).or_default().files.push((index, file));
    }
    Ok(skins)
}

/// 单个条目 -> (外壳目录, 皮肤 id, 文件在皮肤目录内的相对段)。
/// 仅接受 `[外壳?] skins/<id>/<文件...>`, 外壳至多一层; 其余返回 None。
fn classify(parts: &[String]) -> Option<(Option<String>, String, Vec<String>)> {
    let idx = parts.iter().position(|p| p.eq_ignore_ascii_case("skins"))?;
    if idx > 1 {
        return None;
    }
    let rest = &parts[idx + 1..];
    if rest.len() < 2 {
        return None;
    }
    let shell = (idx == 1).then(|| parts[0].clone());
    Some((shell, rest[0].clone(), rest[1..].to_vec()))
}

fn is_junk_name(name: &str) -> bool {
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini") || name.starts_with("._")
}

/// 皮肤 id 需可用作 Windows 目录名
fn valid_skin_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && !id
            .chars()
            .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// 皮肤目录根级是否直接包含指定文件(skin.json / skin.css)
fn has_root_file(skin: &PendingSkin, name: &str) -> bool {
    skin.files
        .iter()
        .any(|(_, f)| f.len() == 1 && f[0].eq_ignore_ascii_case(name))
}

/// 解压到暂存目录并逐个校验 skin.json, 全部通过后覆盖式提交
fn extract_and_commit<R: io::Read + io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    valid: &[(&String, &PendingSkin)],
    staging: &Path,
    skins_root: &Path,
) -> Result<ImportOutcome, String> {
    let mut imported: Vec<ImportedSkin> = Vec::new();
    for (id, skin) in valid {
        let skin_dir = staging.join(id);
        fs::create_dir_all(&skin_dir).map_err(|e| format!("创建暂存目录失败: {e}"))?;
        for (index, file) in &skin.files {
            let dest = skin_dir.join(file.join("/"));
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
            }
            let mut entry = archive
                .by_index(*index)
                .map_err(|e| format!("读取压缩包条目失败: {e}"))?;
            let mut out = File::create(&dest)
                .map_err(|e| format!("写入文件「{}」失败: {e}", dest.display()))?;
            io::copy(&mut entry, &mut out)
                .map_err(|e| format!("解压文件「{}」失败: {e}", dest.display()))?;
        }
        let name = validate_skin_json(&skin_dir)?;
        imported.push(ImportedSkin {
            id: (*id).clone(),
            name,
        });
    }
    // 提交: 同名旧皮肤整体删除后用暂存目录替换
    for skin in &imported {
        let dest = skins_root.join(&skin.id);
        if dest.exists() {
            fs::remove_dir_all(&dest)
                .map_err(|e| format!("清除旧皮肤「{0}」失败: {e}", skin.id))?;
        }
        fs::rename(staging.join(&skin.id), &dest)
            .map_err(|e| format!("写入皮肤「{0}」失败: {e}", skin.id))?;
    }
    let names: Vec<String> = imported.iter().map(|s| format!("「{}」", s.name)).collect();
    let message = if imported.len() == 1 {
        format!("已导入皮肤{}", names[0])
    } else {
        format!("已导入 {} 个皮肤: {}", imported.len(), names.join("、"))
    };
    Ok(ImportOutcome {
        skins: imported,
        message,
    })
}

/// 校验皮肤目录根级的 skin.json: 可解析、name 非空
fn validate_skin_json(skin_dir: &Path) -> Result<String, String> {
    let path = skin_dir.join("skin.json");
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("读取「{0}」的 skin.json 失败: {e}", skin_dir.display()))?;
    let meta: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("「{0}」的 skin.json 不是有效的 JSON: {e}", skin_dir.display()))?;
    meta.get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .ok_or_else(|| format!("「{0}」的 skin.json 缺少有效的 name 字段", skin_dir.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufWriter, Write};
    use std::path::PathBuf;

    const SKIN_JSON: &str = r#"{"name":"测试皮肤","author":"tester","version":"1.0"}"#;

    fn make_zip(tag: &str, files: &[(&str, &[u8])]) -> PathBuf {
        let dir = crate::asar::temp_root(tag);
        let path = dir.join("skin.zip");
        let file = File::create(&path).unwrap();
        let mut w = zip::ZipWriter::new(BufWriter::new(file));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in files {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
        path
    }

    #[test]
    fn imports_flat_layout() {
        let zip = make_zip(
            "import-flat",
            &[
                ("skins/demo/skin.json", SKIN_JSON.as_bytes()),
                ("skins/demo/skin.css", b":root { --color-panel: red; }"),
                ("skins/demo/preview.png", b"\x89PNG"),
                ("skins/demo/assets/bg.png", b"bg"),
            ],
        );
        let root = crate::asar::temp_root("import-flat-out");
        let out = import_zip_into(&zip, &root).unwrap();
        assert_eq!(out.skins.len(), 1);
        assert_eq!(out.skins[0].id, "demo");
        assert_eq!(out.skins[0].name, "测试皮肤");
        assert!(root.join("demo/skin.json").is_file());
        assert!(root.join("demo/assets/bg.png").is_file());
    }

    #[test]
    fn imports_wrapped_layout() {
        let zip = make_zip(
            "import-wrapped",
            &[
                ("my-pack/skins/demo/skin.json", SKIN_JSON.as_bytes()),
                ("my-pack/skins/demo/skin.css", b"body{}"),
            ],
        );
        let root = crate::asar::temp_root("import-wrapped-out");
        let out = import_zip_into(&zip, &root).unwrap();
        assert_eq!(out.skins[0].id, "demo");
        assert!(root.join("demo/skin.css").is_file());
        // 外壳目录不进入皮肤目录
        assert!(!root.join("my-pack").exists());
    }

    #[test]
    fn imports_multiple_skins() {
        let zip = make_zip(
            "import-multi",
            &[
                ("skins/a/skin.json", SKIN_JSON.as_bytes()),
                ("skins/a/skin.css", b"a"),
                ("skins/b/skin.json", r#"{"name":"B皮"}"#.as_bytes()),
                ("skins/b/skin.css", b"b"),
            ],
        );
        let root = crate::asar::temp_root("import-multi-out");
        let out = import_zip_into(&zip, &root).unwrap();
        assert_eq!(out.skins.len(), 2);
        assert!(out.message.contains("2 个皮肤"));
    }

    #[test]
    fn rejects_layout_without_skins_dir() {
        let zip = make_zip(
            "import-noskins",
            &[
                ("demo/skin.json", SKIN_JSON.as_bytes()),
                ("demo/skin.css", b"x"),
            ],
        );
        let root = crate::asar::temp_root("import-noskins-out");
        assert!(import_zip_into(&zip, &root).is_err());
    }

    #[test]
    fn rejects_skin_missing_css() {
        let zip = make_zip("import-nocss", &[("skins/demo/skin.json", SKIN_JSON.as_bytes())]);
        let root = crate::asar::temp_root("import-nocss-out");
        assert!(import_zip_into(&zip, &root).is_err());
    }

    #[test]
    fn rejects_mixed_shells() {
        let zip = make_zip(
            "import-mixed",
            &[
                ("a/skins/x/skin.json", SKIN_JSON.as_bytes()),
                ("a/skins/x/skin.css", b"x"),
                ("b/skins/y/skin.json", SKIN_JSON.as_bytes()),
                ("b/skins/y/skin.css", b"y"),
            ],
        );
        let root = crate::asar::temp_root("import-mixed-out");
        assert!(import_zip_into(&zip, &root).is_err());
    }

    #[test]
    fn overwrites_existing_skin() {
        let zip = make_zip(
            "import-overwrite",
            &[
                ("skins/demo/skin.json", SKIN_JSON.as_bytes()),
                ("skins/demo/skin.css", b"new"),
            ],
        );
        let root = crate::asar::temp_root("import-overwrite-out");
        fs::create_dir_all(root.join("demo")).unwrap();
        fs::write(root.join("demo/old.txt"), b"old").unwrap();
        import_zip_into(&zip, &root).unwrap();
        assert!(root.join("demo/skin.css").is_file());
        // 旧皮肤被整体替换, 不残留旧文件
        assert!(!root.join("demo/old.txt").exists());
    }

    #[test]
    fn skips_dangerous_and_junk_entries() {
        let zip = make_zip(
            "import-junk",
            &[
                ("../evil.txt", b"evil"),
                ("__MACOSX/demo/._skin.json", b"junk"),
                ("skins/demo/skin.json", SKIN_JSON.as_bytes()),
                ("skins/demo/skin.css", b"x"),
                ("skins/demo/.DS_Store", b"junk"),
            ],
        );
        let root = crate::asar::temp_root("import-junk-out");
        let out = import_zip_into(&zip, &root).unwrap();
        assert_eq!(out.skins.len(), 1);
        assert!(!root.join("demo/.DS_Store").exists());
    }

    #[test]
    fn rejects_invalid_skin_json() {
        let zip = make_zip(
            "import-badjson",
            &[
                ("skins/demo/skin.json", b"{ not json"),
                ("skins/demo/skin.css", b"x"),
            ],
        );
        let root = crate::asar::temp_root("import-badjson-out");
        assert!(import_zip_into(&zip, &root).is_err());
        // 校验失败不留半成品
        assert!(!root.join("demo").exists());
    }

    #[test]
    fn rejects_non_zip_extension() {
        let root = crate::asar::temp_root("import-ext-out");
        let file = root.join("skin.7z");
        fs::write(&file, b"x").unwrap();
        assert!(import_zip_into(&file, &root).is_err());
    }
}
