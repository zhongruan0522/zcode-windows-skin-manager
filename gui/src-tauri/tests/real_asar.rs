//! 针对仓库 ZCode/ 快照中真实 app.asar 的集成测试。
//! 只在临时目录里操作副本, 不触碰真实安装目录; 快照缺失时自动跳过。

use std::fs;
use std::path::{Path, PathBuf};
use zcode_skin_manager_lib::asar;
use zcode_skin_manager_lib::inject;

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = gui/src-tauri, 向上两级即仓库根
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("仓库根目录")
}

fn snapshot_file(name: &str) -> Option<PathBuf> {
    let p = repo_root().join("ZCode").join("resources").join(name);
    p.is_file().then_some(p)
}

fn temp_target(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("zsm-it-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(d.join("resources")).unwrap();
    d
}

fn read_inner_file(asar_path: &Path, relpath: &str) -> Vec<u8> {
    let mut f = fs::File::open(asar_path).unwrap();
    let (header, data_start) = asar::parse_header(&mut f).unwrap();
    asar::read_inner(&mut f, &header, data_start, relpath)
        .unwrap()
        .expect(relpath)
}

#[test]
fn real_asar_install_swap_restore_cycle() {
    let Some(orig) = snapshot_file("app.asar.orig") else {
        eprintln!("跳过: 仓库 ZCode/ 快照中没有 app.asar.orig");
        return;
    };
    let target = temp_target("cycle");
    let asar_path = target.join("resources").join("app.asar");
    println!("拷贝真实 asar 副本 (约 281MB)...");
    fs::copy(&orig, &asar_path).unwrap();

    // 原始包: 未注入
    let st = inject::status(&target);
    assert!(st.asar_exists, "副本应存在");
    assert!(
        !st.has_backup && st.installed_skin_id.is_none(),
        "初始应为未安装: {st:?}"
    );

    // 注入液态玻璃
    inject::install_flow(&target, "liquid-glass").expect("install 失败");
    let st = inject::status(&target);
    assert_eq!(st.installed_skin_id.as_deref(), Some("liquid-glass"));
    assert_eq!(st.installed_skin_name.as_deref(), Some("液态玻璃"));
    assert!(st.has_backup);

    // index.html 恰好一条注入 link, 且在 </head> 前
    let html = String::from_utf8(read_inner_file(&asar_path, inject::HTML_RELPATH)).unwrap();
    assert_eq!(html.matches(inject::MARKER).count(), 1);
    assert!(html.find(inject::MARKER).unwrap() < html.find("</head>").unwrap());
    // 皮肤 css 写入且带 id 标记
    let css = read_inner_file(&asar_path, inject::CSS_RELPATH);
    assert!(css.starts_with(b"/* zcode-skin-id: liquid-glass */"));
    // 原有文件仍在(抽样一个非注入路径可读)
    assert!(!read_inner_file(&asar_path, inject::HTML_RELPATH).is_empty());

    // 直接换肤不先还原
    inject::install_flow(&target, "transparent-test").expect("换肤失败");
    let st = inject::status(&target);
    assert_eq!(st.installed_skin_id.as_deref(), Some("transparent-test"));

    // 还原后与官方原始包逐字节一致
    inject::restore_flow(&target).expect("restore 失败");
    let st = inject::status(&target);
    assert!(st.installed_skin_id.is_none() && !st.has_backup);
    assert_eq!(
        fs::read(&asar_path).unwrap().len(),
        fs::read(&orig).unwrap().len(),
        "还原后长度应与原始包一致"
    );
    assert_eq!(fs::read(&asar_path).unwrap(), fs::read(&orig).unwrap());
    fs::remove_dir_all(&target).ok();
}

/// 快照里的 app.asar 是命令行版(Python)注入过的包(无 id 标记),
/// 验证 GUI 的 status 能按 css 内容识别出旧版注入的是哪个内置皮肤。
#[test]
fn real_asar_detects_legacy_python_injection() {
    let Some(injected) = snapshot_file("app.asar") else {
        eprintln!("跳过: 仓库 ZCode/ 快照中没有 app.asar");
        return;
    };
    let html = String::from_utf8(read_inner_file(&injected, inject::HTML_RELPATH)).unwrap();
    if !html.contains(inject::MARKER) {
        eprintln!("跳过: 快照 app.asar 当前未被命令行版注入");
        return;
    }
    let target = injected.parent().unwrap().parent().unwrap().to_path_buf();
    let st = inject::status(&target);
    assert!(
        st.installed_skin_id.is_some(),
        "应识别出命令行版注入的皮肤: {st:?}"
    );
    assert_ne!(
        st.installed_skin_name.as_deref(),
        Some("未知皮肤"),
        "内置皮肤应能按 css 内容匹配: {st:?}"
    );
}
