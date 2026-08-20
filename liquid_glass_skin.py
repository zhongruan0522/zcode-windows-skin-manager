#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
ZCode 桌面版 · 液态玻璃皮肤注入器
================================

原理:
  Electron 应用资源都打包在 resources/app.asar 里。本脚本直接改写 asar:
    1. 在 out/renderer/index.html 的 </head> 前追加一个 <link>, 指向皮肤样式表
    2. 把皮肤样式表作为 out/renderer/assets/liquid-glass.css 写入 asar
    3. 替换/新增文件会重算 SHA256 integrity 哈希, 保证包结构自洽
  原 app.asar 会备份为 app.asar.orig, 随时可以用 restore 命令还原。

用法:
  python liquid_glass_skin.py install                    # 注入到默认位置 C:\\Program Files\\ZCode
  python liquid_glass_skin.py install --target "D:\\xxx" # 注入到其他安装目录(该目录下需有 resources\\app.asar)
  python liquid_glass_skin.py install --css-file my.css  # 使用自定义皮肤 CSS(默认内置液态玻璃)
  python liquid_glass_skin.py restore                    # 从备份还原官方 app.asar
  python liquid_glass_skin.py status                     # 查看注入状态

说明:
  * 写入 Program Files 需要管理员权限, 脚本检测到权限不足会自动弹出 UAC 提权重启自己。
  * 注入/还原前请先完全退出 ZCode 桌面版(脚本也会检测进程)。
  * 应用自动更新后皮肤会被新包覆盖, 重新执行 install 即可。
"""

import argparse
import ctypes
import hashlib
import json
import os
import random
import shutil
import struct
import subprocess
import sys

DEFAULT_TARGET = r"C:\Program Files\ZCode"
ASAR_REL = os.path.join("resources", "app.asar")
BACKUP_SUFFIX = ".orig"

HTML_RELPATH = "out/renderer/index.html"
CSS_RELPATH = "out/renderer/assets/liquid-glass.css"
MARKER = "liquid-glass-skin"  # index.html 里的注入标记, 同时也是 css 文件名
BLOCK_SIZE = 4194304  # asar integrity 分块大小 (4 MiB)

# ============================================================
# 【皮肤开发指南】—— 不看 ZCode 源码也能快速设计皮肤
# ============================================================
# 一、ZCode 桌面版的渲染结构(结论, 直接用)
#   Electron + Vite + React + Tailwind CSS v4:
#   * 主窗口页面: out/renderer/index.html, React 应用挂在 #root
#   * 全部官方样式打包在 out/renderer/assets/styles-*.css (单文件)
#   * 本注入器做的事 = 在 index.html 里追加一个 <link>, 后加载你的皮肤
#     CSS(物理路径 out/renderer/assets/liquid-glass.css), 用它覆盖官方样式
#
# 二、为什么"改几个 CSS 变量"就能全局换肤
#   ZCode 界面颜色不写死, 全部走"语义 token"(CSS 自定义属性), 例如:
#       .bg-panel 这样的工具类  实际 =  background-color: var(--color-panel)
#   浅色值定义在 :root { ... }, 深色值定义在 .dark { ... }。
#   皮肤的核心动作 = 用同名变量给出新值, 整个 app 跟着变。
#
# 三、优先级理论(为什么皮肤能压过官方样式)
#   1. Tailwind v4 的工具类全部位于 @layer 中, 而注入的皮肤是"未分层"
#      样式。CSS 规定: 未分层规则 > 任何 @layer 规则, 同名属性直接赢,
#      所以绝大多数覆盖根本不需要 !important。
#   2. 同优先级时后加载者赢 —— 注入的 <link> 位于官方 <link> 之后。
#   3. 特异性陷阱: .bg-background (0,1,0) 高于 body/html (0,0,1),
#      想覆盖挂在 body/html 上的背景必须 !important 或同样用类选择器。
#
# 四、语义 token 速查表(基于 v3.8.1 实测)
#   背景/表面:
#     --color-background         主窗口/页面底色      (bg-background)
#     --color-background-alt     次级页面底色
#     --color-background-win-alt Windows 布局替代底色
#     --color-header             标题栏               (bg-header)
#     --color-panel              右侧/设置等面板      (bg-panel)
#     --color-sidebar            侧栏                 (bg-sidebar)
#     --color-card/-selected     卡片 / 选中卡片      (bg-card)
#     --color-popover/-header    弹出菜单、下拉浮层   (bg-popover)
#     --color-input/-focused     输入框               (bg-input)
#     --color-surface/-hover     小表面 / 悬停高亮    (bg-surface)
#     --color-terminal-bg        内嵌终端背景
#     --color-hover/-selected    通用悬停/选中态
#   边框:
#     --color-border/-hover      全局边框色
#     --color-card-border / --color-popover-border / --color-input-border(-hover/-focused)
#   文字(建议不动, 保证可读性):
#     --color-foreground / -subtle / -subtlest / -inverse
#   品牌色(可改):
#     --color-brand / --color-accent
#   其余细粒度 token 还有: diff-*, git-*, find-highlight(-active),
#   file-node, command-node, terminal-(fg/cursor/selection/黑红绿...) 等;
#   完整清单: 解包后在 out/renderer/assets/styles-*.css 里搜 ':root' 与 '.dark'。
#
# 五、玻璃/透明效果三要点
#   1. 透明: token 值写成带 alpha 的颜色, 如 rgba(20,25,36,.55)。
#   2. 毛玻璃: backdrop-filter 必须挂在"承载背景色的元素"上。token 只是
#      颜色, 不携带 filter, 因此要重声明对应工具类来补:
#          .bg-panel { backdrop-filter: blur(18px) saturate(165%); }
#   3. 透出去看到什么? Electron 窗口本身是不透明纯色(主进程的
#      BrowserWindow backgroundColor)。想透出"极光/壁纸", 必须自己往
#      html/body 画一层不透明背景(见 SKIN_CSS 第 0 节), 面板半透明+模糊
#      后折射的就是这层自绘背景。
#   性能: blur 元素越多 GPU 开销越大; 大容器用全档, 小元素用约 0.55 档。
#   陷阱: 带 backdrop-filter 的元素会成为 position:fixed 后代的包含块,
#         若个别悬浮件错位, 缩小 backdrop-filter 的作用范围。
#
# 六、深浅主题适配
#   官方通过给 <html> 挂/摘 .dark 类切换主题。皮肤里同时写:
#       :root { 浅色值 }  和  .dark { 深色值 }
#   用户切换主题时皮肤自动跟随(.dark 挂在 html 或 body 上都有效,
#   变量会沿 DOM 继承下去)。
#
# 七、开发调试循环
#   1. 改下面 SKIN_CSS, 或单独写个 my.css;
#   2. python liquid_glass_skin.py install --css-file my.css  (幂等, 随便重跑);
#   3. 完全退出并重启 ZCode 看效果, 不满意回到 1;
#   4. 最小示例参考同目录 skin_transparent_test.css(全透明验证皮肤)。
#
# 八、边界与已知限制
#   * 只皮肤化主窗口 index.html; 进程监控等二级窗口未处理。
#   * 官方自动更新会重写 app.asar, 皮肤随之被覆盖, 重跑 install 即可。
#   * 改文字类 token 前先截图对比, 透明背景上极容易不可读。
# ============================================================
SKIN_CSS = """\
/* ============================================================
 * ZCode · Liquid Glass Skin (液态玻璃)
 * 可调参数: --lg-blur 毛玻璃模糊半径 / --lg-saturate 背景饱和度
 * ============================================================ */
:root {
  --lg-blur: 18px;
  --lg-saturate: 165%;
}

/* ---------- 0. 窗口底色 + 极光背景层 ---------- */
html {
  background: #dfe6f2 !important;
}
html.dark {
  background: #070a12 !important;
}
body {
  background:
    radial-gradient(1100px 720px at 8% -12%, rgba(56, 189, 248, 0.20), transparent 60%),
    radial-gradient(900px 620px at 104% 12%, rgba(129, 140, 248, 0.16), transparent 55%),
    radial-gradient(1000px 780px at 50% 116%, rgba(45, 212, 191, 0.13), transparent 60%),
    linear-gradient(180deg, #eef2f9 0%, #e0e7f3 100%) !important;
  background-attachment: fixed !important;
}
html.dark body {
  background:
    radial-gradient(1100px 720px at 8% -12%, rgba(56, 189, 248, 0.16), transparent 60%),
    radial-gradient(900px 620px at 104% 12%, rgba(129, 140, 248, 0.15), transparent 55%),
    radial-gradient(1000px 780px at 50% 116%, rgba(45, 212, 191, 0.10), transparent 60%),
    linear-gradient(180deg, #0b0f1a 0%, #070a12 100%) !important;
  background-attachment: fixed !important;
}

/* ---------- 1. 语义色 token -> 半透明玻璃 ---------- */
:root {
  --color-background: rgba(255, 255, 255, 0.40);
  --color-background-alt: rgba(255, 255, 255, 0.50);
  --color-background-win-alt: rgba(255, 255, 255, 0.55);
  --color-header: rgba(255, 255, 255, 0.45);
  --color-panel: rgba(255, 255, 255, 0.55);
  --color-sidebar: rgba(255, 255, 255, 0.42);
  --color-card: rgba(255, 255, 255, 0.55);
  --color-card-selected: rgba(255, 255, 255, 0.72);
  --color-card-border: rgba(15, 23, 42, 0.10);
  --color-popover: rgba(255, 255, 255, 0.74);
  --color-popover-header: rgba(255, 255, 255, 0.55);
  --color-popover-border: rgba(15, 23, 42, 0.10);
  --color-input: rgba(255, 255, 255, 0.55);
  --color-input-focused: rgba(255, 255, 255, 0.72);
  --color-surface: rgba(255, 255, 255, 0.42);
  --color-surface-hover: rgba(255, 255, 255, 0.60);
  --color-border: rgba(15, 23, 42, 0.10);
  --color-border-hover: rgba(15, 23, 42, 0.18);
  --color-terminal-bg: rgba(255, 255, 255, 0.68);
}
.dark {
  --color-background: rgba(18, 22, 32, 0.42);
  --color-background-alt: rgba(20, 24, 34, 0.52);
  --color-background-win-alt: rgba(20, 24, 34, 0.55);
  --color-header: rgba(16, 20, 30, 0.45);
  --color-panel: rgba(20, 25, 36, 0.55);
  --color-sidebar: rgba(13, 16, 24, 0.45);
  --color-card: rgba(24, 29, 41, 0.55);
  --color-card-selected: rgba(35, 41, 56, 0.68);
  --color-card-border: rgba(255, 255, 255, 0.14);
  --color-popover: rgba(22, 27, 39, 0.80);
  --color-popover-header: rgba(30, 36, 50, 0.60);
  --color-popover-border: rgba(255, 255, 255, 0.16);
  --color-input: rgba(255, 255, 255, 0.08);
  --color-input-focused: rgba(255, 255, 255, 0.13);
  --color-surface: rgba(255, 255, 255, 0.07);
  --color-surface-hover: rgba(255, 255, 255, 0.13);
  --color-hover: rgba(255, 255, 255, 0.10);
  --color-selected: rgba(255, 255, 255, 0.16);
  --color-border: rgba(255, 255, 255, 0.14);
  --color-border-hover: rgba(255, 255, 255, 0.26);
  --color-terminal-bg: rgba(10, 13, 20, 0.85);
}

/* ---------- 2. 玻璃工具类: 挂上 backdrop-filter ----------
 * 结构层(侧栏/面板/标题栏/弹层)用全档模糊;
 * 元素层(卡片/输入框/表面)用约 0.55 档, 避免"糊成一片"。 */
.bg-panel, .bg-sidebar, .bg-header, .bg-popover {
  -webkit-backdrop-filter: blur(var(--lg-blur)) saturate(var(--lg-saturate));
  backdrop-filter: blur(var(--lg-blur)) saturate(var(--lg-saturate));
}
.bg-card, .bg-input, .bg-surface, .bg-terminal-bg {
  -webkit-backdrop-filter: blur(calc(var(--lg-blur) * 0.55)) saturate(var(--lg-saturate));
  backdrop-filter: blur(calc(var(--lg-blur) * 0.55)) saturate(var(--lg-saturate));
}

/* ---------- 3. 边缘高光 / 投影 (主题中性的白描边) ---------- */
.bg-panel, .bg-sidebar, .bg-header {
  box-shadow:
    inset 0 1px 0 rgba(255, 255, 255, 0.30),
    inset 0 0 0 1px rgba(255, 255, 255, 0.05);
}
.bg-popover {
  box-shadow:
    0 18px 48px -12px rgba(2, 6, 23, 0.35),
    0 2px 8px -2px rgba(2, 6, 23, 0.18),
    inset 0 1px 0 rgba(255, 255, 255, 0.28);
}
.bg-card {
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.22);
}

/* ---------- 4. 细节: 选区 / 滚动条 ---------- */
::selection {
  background: rgba(56, 189, 248, 0.32);
}
::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}
::-webkit-scrollbar-track,
::-webkit-scrollbar-corner {
  background: transparent;
}
::-webkit-scrollbar-thumb {
  background-color: rgba(125, 138, 160, 0.35);
  border-radius: 999px;
  border: 2px solid transparent;
  background-clip: padding-box;
}
::-webkit-scrollbar-thumb:hover {
  background-color: rgba(125, 138, 160, 0.55);
}

/* ---------- 5. 关闭透明效果的系统偏好时, 回退到高不透明度 ---------- */
@media (prefers-reduced-transparency: reduce) {
  :root {
    --color-background: rgba(255, 255, 255, 0.96);
    --color-panel: rgba(255, 255, 255, 0.97);
    --color-sidebar: rgba(255, 255, 255, 0.96);
    --color-card: rgba(255, 255, 255, 0.97);
    --color-popover: rgba(255, 255, 255, 0.99);
  }
  .dark {
    --color-background: rgba(18, 22, 32, 0.96);
    --color-panel: rgba(20, 25, 36, 0.97);
    --color-sidebar: rgba(13, 16, 24, 0.96);
    --color-card: rgba(24, 29, 41, 0.97);
    --color-popover: rgba(22, 27, 39, 0.99);
  }
  .bg-panel, .bg-sidebar, .bg-header, .bg-popover,
  .bg-card, .bg-input, .bg-surface, .bg-terminal-bg {
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
  }
}
"""


# ============================================================
# asar 读写 (纯标准库)
# 文件布局:
#   [u32 sizePickle载荷长=4][u32 header段长][u32 header载荷长][u32 json长]
#   [json 头][按 json 偏移排列的文件数据]
# ============================================================
class AsarError(Exception):
    pass


def parse_header(f):
    """读取并校验 asar 头, 返回 (header_dict, data_start)。f 需以 'rb' 打开。"""
    head = f.read(16)
    if len(head) < 16:
        raise AsarError("文件太小, 不是有效的 asar 包")
    sz_pickle_len, header_size, payload_size, json_len = struct.unpack("<IIII", head)
    pad = payload_size - 4 - json_len  # pickle 按 4 字节对齐的填充, 0~3 字节
    if sz_pickle_len != 4 or header_size != payload_size + 4 or not 0 <= pad <= 3:
        raise AsarError("asar 头部结构不符合预期 (可能是新版格式)")
    f.seek(16)
    header = json.loads(f.read(json_len).decode("utf-8"))
    if "files" not in header:
        raise AsarError("asar 头部缺少 files 字段")
    # 物理布局: [sizePickle 8字节][headerPickle: 载荷长u32+载荷] [数据区]
    # header_size = headerPickle 总长(含自身4字节), 数据区起点 = 8 + header_size
    return header, 8 + header_size


def get_entry(header, relpath):
    node = header
    for part in relpath.split("/"):
        if "files" not in node or part not in node["files"]:
            return None
        node = node["files"][part]
    return node


def set_entry(header, relpath, offset, size, with_integrity, content):
    parts = relpath.split("/")
    node = header
    for part in parts[:-1]:
        node = node.setdefault("files", {}).setdefault(part, {"files": {}})
    entry = {"size": size, "offset": str(offset)}
    if with_integrity:
        entry["integrity"] = make_integrity(content)
    node.setdefault("files", {})[parts[-1]] = entry


def make_integrity(content):
    h = hashlib.sha256(content).hexdigest()
    blocks = [
        hashlib.sha256(content[i:i + BLOCK_SIZE]).hexdigest()
        for i in range(0, len(content), BLOCK_SIZE)
    ]
    return {"algorithm": "SHA256", "hash": h, "blockSize": BLOCK_SIZE, "blocks": blocks}


def read_inner(f, header, data_start, relpath):
    entry = get_entry(header, relpath)
    if entry is None or "offset" not in entry:
        return None
    f.seek(data_start + int(entry["offset"]))
    return f.read(entry["size"])


def build_patched_asar(src_asar, out_path, replacements, additions):
    """以 src_asar 为基础重建 asar:
    原数据区原样拷贝(旧 offset 全部有效), 修改/新增的内容追加在数据区末尾。"""
    with open(src_asar, "rb") as f:
        header, data_start = parse_header(f)
        f.seek(0, os.SEEK_END)
        data_len = f.tell() - data_start

        blobs = []
        cursor = data_len

        def place(relpath, content):
            nonlocal cursor
            old = get_entry(header, relpath)
            set_entry(header, relpath, cursor, len(content),
                      with_integrity=(old is not None and "integrity" in old) or relpath in additions,
                      content=content)
            blobs.append(content)
            cursor += len(content)

        for relpath, content in list(replacements.items()) + list(additions.items()):
            place(relpath, content)

        header_json = json.dumps(header, separators=(",", ":")).encode("utf-8")
        payload = struct.pack("<I", len(header_json)) + header_json
        payload += b"\0" * (-len(payload) % 4)
        header_buf = struct.pack("<I", len(payload)) + payload

        with open(out_path, "wb") as out:
            out.write(struct.pack("<II", 4, len(header_buf)))  # sizePickle: [载荷长=4][header段长]
            out.write(header_buf)
            f.seek(data_start)
            remaining = data_len
            while remaining > 0:  # 精确拷贝数据区, 不多不少
                chunk = f.read(min(1024 * 1024, remaining))
                if not chunk:
                    raise AsarError(f"源 asar 数据区比预期短了 {remaining} 字节")
                out.write(chunk)
                remaining -= len(chunk)
            for b in blobs:
                out.write(b)

        expected = 8 + len(header_buf) + data_len + sum(len(b) for b in blobs)
        if os.path.getsize(out_path) != expected:
            raise AsarError(f"输出长度不符: {os.path.getsize(out_path)} != 预期 {expected}")

    verify_asar(out_path, {**replacements, **additions})


def verify_asar(path, expect_files, sample_count=8):
    """重开新包: 校验注入文件内容 + 抽样校验原有文件哈希仍然对得上。"""
    with open(path, "rb") as f:
        header, data_start = parse_header(f)
        for relpath, content in expect_files.items():
            got = read_inner(f, header, data_start, relpath)
            if got != content:
                raise AsarError(f"校验失败: {relpath} 内容不一致")

        # 随机抽样原有条目, 用 integrity 哈希验证数据区没有错位
        entries = []

        def walk(node, prefix):
            for name, e in node.get("files", {}).items():
                p = f"{prefix}/{name}" if prefix else name
                if "offset" in e:
                    entries.append((p, e))
                walk(e, p)

        walk(header, "")
        random.seed(42)
        for relpath, e in random.sample(entries, min(sample_count, len(entries))):
            if "integrity" not in e:
                continue
            f.seek(data_start + int(e["offset"]))
            got = f.read(e["size"])
            if hashlib.sha256(got).hexdigest() != e["integrity"]["hash"]:
                raise AsarError(f"校验失败: 抽样文件 {relpath} 哈希不匹配, 数据区可能错位")


# ============================================================
# 注入 / 还原 / 状态
# ============================================================
def zcode_running_under(target_dir):
    """只检测"目标目录下"的 ZCode.exe 是否在运行(其他目录的实例不占用目标的 asar)。"""
    try:
        out = subprocess.run(
            ["powershell", "-NoProfile", "-Command",
             "(Get-Process ZCode -ErrorAction SilentlyContinue).Path"],
            capture_output=True, text=True,
        ).stdout
    except Exception:
        return False
    target_dir = os.path.normcase(os.path.abspath(target_dir))
    for line in out.splitlines():
        line = line.strip()
        if line and os.path.normcase(os.path.abspath(os.path.dirname(line))).startswith(target_dir + os.sep):
            return True
    return False


def inject_link(html_text):
    if MARKER in html_text:
        return None  # 已注入过
    link = (f'<!--{MARKER}-->'
            f'<link rel="stylesheet" crossorigin href="./assets/liquid-glass.css">')
    if "</head>" not in html_text:
        raise AsarError("index.html 中找不到 </head>")
    return html_text.replace("</head>", link + "</head>", 1)


def cmd_install(args):
    target = os.path.abspath(args.target)
    asar = os.path.join(target, ASAR_REL)
    if not os.path.isfile(asar):
        die(f"找不到 {asar}\n请用 --target 指定 ZCode 安装目录")

    if zcode_running_under(target):
        die("目标目录下的 ZCode.exe 正在运行, 请先完全退出 ZCode 桌面版再注入。")

    backup = asar + BACKUP_SUFFIX
    if not os.path.exists(backup):
        try:
            shutil.copy2(asar, backup)
            print(f"[1/5] 已备份原始包 -> {backup}")
        except PermissionError:
            elevate_self(args)
            return
    else:
        print(f"[1/5] 备份已存在, 复用 {backup}")

    # 始终以原始备份为底稿重建, 天然幂等
    with open(backup, "rb") as f:
        header, data_start = parse_header(f)
        html_bytes = read_inner(f, header, data_start, HTML_RELPATH)
    if html_bytes is None:
        die(f"asar 中找不到 {HTML_RELPATH}")
    html_text = html_bytes.decode("utf-8")
    new_html = inject_link(html_text)
    if new_html is None:
        new_html = html_text  # 已注入过, 保留现有 <link>, 只重建 css 内容

    css = SKIN_CSS
    if args.css_file:
        with open(args.css_file, "r", encoding="utf-8") as cf:
            css = cf.read()

    print("[2/5] 重建 asar (原数据区原样保留, 注入内容追加至末尾)...")
    tmp = asar + ".lgtmp"
    build_patched_asar(
        backup, tmp,
        replacements={HTML_RELPATH: new_html.encode("utf-8")},
        additions={CSS_RELPATH: css.encode("utf-8")},
    )
    print("[3/5] 新包自检通过 (注入内容 + 抽样哈希校验)")

    try:
        os.replace(tmp, asar)
    except PermissionError:
        elevate_self(args)
        return
    print("[4/5] 已写回 app.asar")
    print("[5/5] 完成! 重启 ZCode 桌面版即可看到液态玻璃效果。")
    print("     不喜欢就 restore 一键还原; 应用自动更新后需重新 install。")


def cmd_restore(args):
    target = os.path.abspath(args.target)
    asar = os.path.join(target, ASAR_REL)
    backup = asar + BACKUP_SUFFIX
    if not os.path.isfile(backup):
        die(f"找不到备份 {backup}, 无需还原或备份已丢失。")
    if zcode_running_under(target):
        die("目标目录下的 ZCode.exe 正在运行, 请先完全退出 ZCode 桌面版再还原。")
    try:
        os.replace(backup, asar)
    except PermissionError:
        elevate_self(args)
        return
    print("已还原官方 app.asar, 重启 ZCode 即恢复原皮肤。")


def cmd_status(args):
    target = os.path.abspath(args.target)
    asar = os.path.join(target, ASAR_REL)
    if not os.path.isfile(asar):
        die(f"找不到 {asar}")
    with open(asar, "rb") as f:
        header, data_start = parse_header(f)
        html = read_inner(f, header, data_start, HTML_RELPATH)
        css = read_inner(f, header, data_start, CSS_RELPATH)
    installed = html is not None and MARKER in html.decode("utf-8", "ignore")
    skin_name = "未安装"
    if installed:
        skin_name = ("液态玻璃 (内置)" if css == SKIN_CSS.encode("utf-8")
                     else "自定义皮肤 (--css-file 装的)")
    has_backup = os.path.isfile(asar + BACKUP_SUFFIX)
    print(f"目标:   {asar}")
    print(f"皮肤:   {skin_name}")
    print(f"备份:   {'app.asar.orig 存在, 可还原' if has_backup else '无备份'}")


# ============================================================
# 提权 & 命令行入口
# ============================================================
def is_admin():
    try:
        return ctypes.windll.shell32.IsUserAnAdmin() != 0
    except Exception:
        return False


def elevate_self(args):
    """用 UAC 重新启动自己(管理员), 仅用于写 Program Files 等受保护目录。"""
    if getattr(args, "elevated", False):
        die("当前已是管理员但仍无法写入, 请检查目录权限或杀毒软件。")
    script = os.path.abspath(__file__)
    cmd = [sys.executable, script, args.command, "--target", os.path.abspath(args.target), "--elevated"]
    print("目标目录需要管理员权限, 正在请求提权 (请在弹出的 UAC 窗口点“是”)...")
    ret = ctypes.windll.shell32.ShellExecuteW(
        None, "runas", sys.executable, subprocess.list2cmdline(cmd), None, 1)
    if ret <= 32:
        die("提权被取消, 未做任何修改。")
    sys.exit(0)


def die(msg):
    print(f"错误: {msg}")
    sys.exit(1)


def main():
    ap = argparse.ArgumentParser(description="ZCode 液态玻璃皮肤注入器")
    ap.add_argument("command", choices=["install", "restore", "status"],
                    help="install=注入皮肤  restore=还原  status=查看状态")
    ap.add_argument("--target", default=DEFAULT_TARGET,
                    help=f"ZCode 安装目录 (默认 {DEFAULT_TARGET})")
    ap.add_argument("--css-file", help="使用自定义 CSS 文件替换内置皮肤")
    ap.add_argument("--elevated", action="store_true", help=argparse.SUPPRESS)
    args = ap.parse_args()

    if os.name != "nt":
        die("本脚本仅支持 Windows。")

    handlers = {"install": cmd_install, "restore": cmd_restore, "status": cmd_status}
    handlers[args.command](args)

    if args.elevated:
        input("\n按回车键关闭窗口...")


if __name__ == "__main__":
    main()
