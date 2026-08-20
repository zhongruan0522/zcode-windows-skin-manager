//! asar 读写与 integrity 重算, 移植自 liquid_glass_skin.py 的纯标准库实现。
//!
//! 文件布局:
//!   [u32 sizePickle载荷长=4][u32 header段长][u32 header载荷长][u32 json长]
//!   [json 头][按 json 偏移排列的文件数据]

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

/// asar integrity 分块大小 (4 MiB)
pub const BLOCK_SIZE: usize = 4 * 1024 * 1024;

pub type AsarResult<T> = Result<T, String>;

/// 读取并校验 asar 头, 返回 (header JSON, 数据区起始偏移)
pub fn parse_header(f: &mut (impl Read + Seek)) -> AsarResult<(Value, u64)> {
    let mut head = [0u8; 16];
    f.read_exact(&mut head)
        .map_err(|_| "文件太小, 不是有效的 asar 包".to_string())?;
    let sz_pickle_len = u32::from_le_bytes(head[0..4].try_into().unwrap()) as usize;
    let header_size = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    let payload_size = u32::from_le_bytes(head[8..12].try_into().unwrap()) as usize;
    let json_len = u32::from_le_bytes(head[12..16].try_into().unwrap()) as usize;
    let pad = payload_size
        .checked_sub(4)
        .and_then(|v| v.checked_sub(json_len))
        .ok_or_else(|| "asar 头部结构不符合预期".to_string())?;
    if sz_pickle_len != 4 || header_size != payload_size + 4 || pad > 3 {
        return Err("asar 头部结构不符合预期 (可能是新版格式)".into());
    }
    f.seek(SeekFrom::Start(16)).map_err(|e| e.to_string())?;
    let mut json_buf = vec![0u8; json_len];
    f.read_exact(&mut json_buf).map_err(|e| e.to_string())?;
    let header: Value =
        serde_json::from_slice(&json_buf).map_err(|e| format!("asar 头 JSON 解析失败: {e}"))?;
    if header.get("files").is_none() {
        return Err("asar 头部缺少 files 字段".into());
    }
    Ok((header, (8 + header_size) as u64))
}

pub fn get_entry<'a>(header: &'a Value, relpath: &str) -> Option<&'a Value> {
    let mut node = header;
    for part in relpath.split('/') {
        node = node.get("files")?.get(part)?;
    }
    Some(node)
}

/// 在 header 中写入/替换一个文件条目（含 integrity 重算）
pub fn set_entry(
    header: &mut Value,
    relpath: &str,
    offset: u64,
    size: u64,
    with_integrity: bool,
    content: &[u8],
) {
    let parts: Vec<&str> = relpath.split('/').collect();
    let mut node: &mut Value = header;
    for part in &parts[..parts.len() - 1] {
        let obj = node.as_object_mut().expect("asar 节点必须是对象");
        obj.entry("files").or_insert_with(|| json!({}));
        let files = node
            .get_mut("files")
            .and_then(|v| v.as_object_mut())
            .expect("files 必为对象");
        files
            .entry(part.to_string())
            .or_insert_with(|| json!({"files": {}}));
        node = node
            .get_mut("files")
            .and_then(|f| f.get_mut(part))
            .expect("刚创建的节点必然存在");
    }
    let parent_files = node
        .as_object_mut()
        .expect("asar 节点必须是对象")
        .entry("files")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("files 必为对象");
    let mut entry = Map::new();
    entry.insert("size".into(), json!(size));
    entry.insert("offset".into(), json!(offset.to_string()));
    if with_integrity {
        entry.insert("integrity".into(), make_integrity(content));
    }
    parent_files.insert(parts[parts.len() - 1].to_string(), Value::Object(entry));
}

pub fn make_integrity(content: &[u8]) -> Value {
    let hash = format!("{:x}", Sha256::digest(content));
    let blocks: Vec<Value> = content
        .chunks(BLOCK_SIZE)
        .map(|c| json!(format!("{:x}", Sha256::digest(c))))
        .collect();
    json!({
        "algorithm": "SHA256",
        "hash": hash,
        "blockSize": BLOCK_SIZE,
        "blocks": blocks,
    })
}

pub fn read_inner(
    f: &mut (impl Read + Seek),
    header: &Value,
    data_start: u64,
    relpath: &str,
) -> io::Result<Option<Vec<u8>>> {
    let Some(entry) = get_entry(header, relpath) else {
        return Ok(None);
    };
    let Some(offset) = entry.get("offset").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let offset: u64 = offset
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "非法 offset"))?;
    let size = entry.get("size").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    f.seek(SeekFrom::Start(data_start + offset))?;
    let mut buf = vec![0u8; size];
    f.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// 序列化 asar 头部 16 字节前缀 + header JSON（pickle 按 4 字节对齐）
fn serialize_header(header: &Value) -> Vec<u8> {
    let json = serde_json::to_vec(header).expect("header 必可序列化");
    let unpadded = 4 + json.len();
    let pad = (4 - unpadded % 4) % 4;
    let padded = (unpadded + pad) as u32;
    let mut buf = Vec::with_capacity(16 + json.len() + pad);
    buf.extend_from_slice(&4u32.to_le_bytes()); // sizePickle 载荷长
    buf.extend_from_slice(&(padded + 4).to_le_bytes()); // header 段总长
    buf.extend_from_slice(&padded.to_le_bytes()); // header 载荷长(含对齐)
    buf.extend_from_slice(&(json.len() as u32).to_le_bytes());
    buf.extend_from_slice(&json);
    buf.extend(std::iter::repeat_n(0u8, pad));
    buf
}

/// 以 src 为基础重建 asar: 原数据区原样拷贝（旧 offset 全部有效），
/// 修改/新增的内容追加在数据区末尾，替换文件保留原 integrity 有无状态。
#[allow(clippy::type_complexity)]
pub fn build_patched_asar(
    src: &Path,
    out_path: &Path,
    replacements: Vec<(String, Vec<u8>)>,
    additions: Vec<(String, Vec<u8>)>,
) -> AsarResult<()> {
    let mut f = File::open(src).map_err(|e| format!("打开 asar 失败: {e}"))?;
    let (mut header, data_start) = parse_header(&mut f)?;
    let src_len = f.metadata().map_err(|e| e.to_string())?.len();
    if src_len < data_start {
        return Err("源 asar 数据区长度异常".into());
    }
    let data_len = src_len - data_start;

    let work: Vec<(String, Vec<u8>, bool)> = replacements
        .into_iter()
        .map(|(p, c)| (p, c, false))
        .chain(additions.into_iter().map(|(p, c)| (p, c, true)))
        .collect();

    let mut blobs: Vec<Vec<u8>> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut cursor = data_len;
    for (relpath, content, is_addition) in work {
        let with_integrity = is_addition
            || get_entry(&header, &relpath)
                .and_then(|e| e.get("integrity"))
                .is_some();
        set_entry(
            &mut header,
            &relpath,
            cursor,
            content.len() as u64,
            with_integrity,
            &content,
        );
        cursor += content.len() as u64;
        names.push(relpath);
        blobs.push(content);
    }

    let header_buf = serialize_header(&header);
    let expect_len =
        header_buf.len() as u64 + data_len + blobs.iter().map(|b| b.len() as u64).sum::<u64>();

    let mut out =
        io::BufWriter::new(File::create(out_path).map_err(|e| format!("创建临时文件失败: {e}"))?);
    out.write_all(&header_buf).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(data_start))
        .map_err(|e| e.to_string())?;
    let mut limited = f.take(data_len);
    io::copy(&mut limited, &mut out).map_err(|e| format!("拷贝数据区失败: {e}"))?;
    for b in &blobs {
        out.write_all(b).map_err(|e| e.to_string())?;
    }
    out.flush().map_err(|e| e.to_string())?;
    let actual_len = out.get_ref().metadata().map_err(|e| e.to_string())?.len();
    drop(out);
    if actual_len != expect_len {
        return Err(format!("输出长度不符: {actual_len} != 预期 {expect_len}"));
    }

    // 重开新包自检: 注入文件内容一致 + 抽样校验原有文件哈希
    let expect_files: Vec<(String, Vec<u8>)> = names.into_iter().zip(blobs).collect();
    verify_asar(out_path, &expect_files, 8)
}

/// 重开 asar: 校验指定文件内容 + 抽样校验原有条目哈希，防止数据区错位
pub fn verify_asar(
    path: &Path,
    expect_files: &[(String, Vec<u8>)],
    sample_count: usize,
) -> AsarResult<()> {
    let mut f = File::open(path).map_err(|e| format!("重开 asar 失败: {e}"))?;
    let (header, data_start) = parse_header(&mut f)?;
    for (relpath, content) in expect_files {
        match read_inner(&mut f, &header, data_start, relpath).map_err(|e| e.to_string())? {
            Some(got) if &got == content => {}
            _ => return Err(format!("校验失败: {relpath} 内容不一致")),
        }
    }
    // 收集带 integrity 的条目，等距抽样验证
    let mut entries: Vec<(String, u64, u64, String)> = Vec::new();
    walk_entries(&header, "", &mut entries);
    if !entries.is_empty() {
        let step = (entries.len() / sample_count).max(1);
        for (p, off, size, hash) in entries.iter().step_by(step).take(sample_count) {
            f.seek(SeekFrom::Start(data_start + off))
                .map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; *size as usize];
            f.read_exact(&mut buf).map_err(|e| e.to_string())?;
            if format!("{:x}", Sha256::digest(&buf)) != *hash {
                return Err(format!("校验失败: 抽样文件 {p} 哈希不匹配, 数据区可能错位"));
            }
        }
    }
    Ok(())
}

fn walk_entries(node: &Value, prefix: &str, out: &mut Vec<(String, u64, u64, String)>) {
    let Some(files) = node.get("files").and_then(|v| v.as_object()) else {
        return;
    };
    for (name, e) in files {
        let p = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        if let (Some(off), Some(size), Some(hash)) = (
            e.get("offset").and_then(|v| v.as_str()),
            e.get("size").and_then(|v| v.as_u64()),
            e.get("integrity")
                .and_then(|i| i.get("hash"))
                .and_then(|h| h.as_str()),
        ) {
            if let Ok(off) = off.parse::<u64>() {
                out.push((p.clone(), off, size, hash.to_string()));
            }
        }
        walk_entries(e, &p, out);
    }
}

// ============================================================
// 测试辅助与单元测试
// ============================================================

/// 唯一临时目录（自动清空已存在的同名目录）
#[cfg(test)]
pub(crate) fn temp_root(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "zsm-test-{}-{}-{}",
        tag,
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// 从零构建一个最小 asar（测试用）
#[cfg(test)]
pub(crate) fn build_test_asar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut header = json!({"files": {}});
    let mut data: Vec<u8> = Vec::new();
    for (relpath, content) in files {
        set_entry(
            &mut header,
            relpath,
            data.len() as u64,
            content.len() as u64,
            true,
            content,
        );
        data.extend_from_slice(content);
    }
    let mut out = serialize_header(&header);
    out.extend_from_slice(&data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_header_rejects_short_file() {
        let mut cur = io::Cursor::new(b"short".to_vec());
        assert!(parse_header(&mut cur).is_err());
    }

    #[test]
    fn parse_header_rejects_bad_magic() {
        let mut cur = io::Cursor::new(vec![0u8; 16]);
        assert!(parse_header(&mut cur).is_err());
    }

    #[test]
    fn parse_header_rejects_inconsistent_sizes() {
        // payload_size 与 json_len 不自洽
        let mut buf = 4u32.to_le_bytes().to_vec();
        buf.extend_from_slice(&100u32.to_le_bytes());
        buf.extend_from_slice(&200u32.to_le_bytes()); // payload=200
        buf.extend_from_slice(&500u32.to_le_bytes()); // json=500 > payload-4
        buf.extend_from_slice(&[0u8; 64]);
        let mut cur = io::Cursor::new(buf);
        assert!(parse_header(&mut cur).is_err());
    }

    #[test]
    fn test_asar_roundtrip_read() {
        let files: Vec<(&str, &[u8])> = vec![
            ("a.txt", b"hello"),
            ("out/renderer/index.html", b"<html></head></html>"),
        ];
        let bytes = build_test_asar(&files);
        let mut cur = io::Cursor::new(&bytes);
        let (header, data_start) = parse_header(&mut cur).unwrap();
        assert_eq!(
            read_inner(&mut cur, &header, data_start, "a.txt")
                .unwrap()
                .unwrap(),
            b"hello"
        );
        assert_eq!(
            read_inner(&mut cur, &header, data_start, "out/renderer/index.html")
                .unwrap()
                .unwrap(),
            b"<html></head></html>"
        );
        assert!(read_inner(&mut cur, &header, data_start, "missing.txt")
            .unwrap()
            .is_none());
    }

    #[test]
    fn patch_replaces_adds_and_keeps_originals() {
        let dir = temp_root("asar-patch");
        let src = dir.join("src.asar");
        let out = dir.join("out.asar");

        // big.bin 5MB: 触发多分块 integrity(2 blocks)
        let big = vec![7u8; 5 * 1024 * 1024];
        let files: Vec<(&str, &[u8])> = vec![
            ("a.txt", b"hello"),
            ("dir/b.txt", b"world world"),
            ("big.bin", &big),
        ];
        fs::write(&src, build_test_asar(&files)).unwrap();

        build_patched_asar(
            &src,
            &out,
            vec![("a.txt".into(), b"HELLO NEW".to_vec())],
            vec![("out/renderer/assets/skin.css".into(), b"body{}".to_vec())],
        )
        .unwrap();

        let mut f = File::open(&out).unwrap();
        let (header, data_start) = parse_header(&mut f).unwrap();
        assert_eq!(
            read_inner(&mut f, &header, data_start, "a.txt")
                .unwrap()
                .unwrap(),
            b"HELLO NEW"
        );
        assert_eq!(
            read_inner(&mut f, &header, data_start, "dir/b.txt")
                .unwrap()
                .unwrap(),
            b"world world"
        );
        assert_eq!(
            read_inner(&mut f, &header, data_start, "big.bin")
                .unwrap()
                .unwrap(),
            big
        );
        assert_eq!(
            read_inner(&mut f, &header, data_start, "out/renderer/assets/skin.css")
                .unwrap()
                .unwrap(),
            b"body{}"
        );
        // 新增条目带 integrity 且分块正确
        let entry = get_entry(&header, "out/renderer/assets/skin.css").unwrap();
        assert_eq!(entry["integrity"]["algorithm"], "SHA256");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn patch_is_idempotent_length_stable() {
        let dir = temp_root("asar-idem");
        let src = dir.join("src.asar");
        let html: &[u8] = b"<html><head></head><body></body></html>";
        fs::write(&src, build_test_asar(&[("out/renderer/index.html", html)])).unwrap();

        let out1 = dir.join("out1.asar");
        build_patched_asar(
            &src,
            &out1,
            vec![("out/renderer/index.html".into(), b"<link>".to_vec())],
            vec![("skin.css".into(), b"abc".to_vec())],
        )
        .unwrap();
        // 以同一底稿再跑一次, 输出应逐字节一致(幂等)
        let out2 = dir.join("out2.asar");
        build_patched_asar(
            &src,
            &out2,
            vec![("out/renderer/index.html".into(), b"<link>".to_vec())],
            vec![("skin.css".into(), b"abc".to_vec())],
        )
        .unwrap();
        assert_eq!(fs::read(&out1).unwrap(), fs::read(&out2).unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
