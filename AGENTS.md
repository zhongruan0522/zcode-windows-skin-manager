# ZCode Windows Skin Manager

给 ZCode 桌面版（Electron 应用）做换肤的工具集。核心原理：改写安装目录下
`resources/app.asar`，在 `out/renderer/index.html` 的 `</head>` 前注入
`<link>` 标签，并把皮肤 CSS 写入 `out/renderer/assets/`。注入前备份
`app.asar.orig`，可随时 restore。ZCode 自动更新后皮肤会被覆盖，重新 install 即可。

## 目录结构

- `liquid_glass_skin.py` — 初版命令行注入器（install / restore / status），
  **待被 GUI 重构取代的参考实现**，asar 逻辑的移植来源，请勿继续维护此文件！！！
- `skins/` — 皮肤包目录（见下方皮肤包规范），仓库级共享数据，不塞进 `gui/`
- `ZCode/` — ZCode 桌面版安装目录的快照/工作区，**不要提交大文件，不要随意改动**
- `gui/` — Tauri 应用（重构目标），自包含，规划结构如下：

```
gui/
├─ package.json              # pnpm 管理
├─ src/                      # 前端
│  ├─ components/            # SkinCard / PreviewModal / StatusBar
│  ├─ views/                 # SkinList / Settings（安装目录路径等）
│  ├─ lib/                   # invoke() 的 TS 封装，类型与 Rust 侧对齐
│  └─ assets/
└─ src-tauri/
   ├─ tauri.conf.json        # 通过 resources 把根目录 skins/ 打进安装包
   └─ src/
      ├─ lib.rs              # command 注册入口
      ├─ asar.rs             # asar 读写 + integrity 重算（从 py 移植）
      ├─ inject.rs           # install / restore / status 流程编排
      ├─ skins.rs            # 扫描皮肤目录、校验 skin.json
      ├─ elevate.rs          # UAC 提权
      ├─ process.rs          # ZCode 进程检测
      └─ paths.rs            # 安装目录解析（注册表/常见位置/进程自动检测）+ 皮肤目录
```

结构约定：

- Rust 模块与 `liquid_glass_skin.py` 中的功能块一一对应，移植时逐块搬、逐块验证
- 开发时后端读仓库根目录 `../skins`，打包时经 `tauri.conf.json` 的
  `resources` 随应用分发
- 运行时另设用户皮肤目录 `%APPDATA%\zcode-skin-manager\skins`，
  `paths.rs` 合并"内置 + 用户"两个来源，同名时用户目录优先
- GUI 代码多起来后可在 `gui/` 内嵌套一份 AGENTS.md 放前端/Rust 的构建命令，
  根目录这份只管项目级约定

## 常用命令

项目处于重构期：命令行注入器 `liquid_glass_skin.py` 是**初版参考实现**，
其 asar 读写 / integrity 重算 / UAC 提权 / 进程检测逻辑是重构时的移植来源，
  不要删。后续以 Tauri GUI 为唯一入口（皮肤浏览预览 → 一键注入/还原），
  GUI 后端直接移植上述逻辑，而不是 shell 调 Python 脚本。

注入/还原前必须完全退出 ZCode 桌面版（逻辑需检测进程）。

## 皮肤包规范

每个皮肤是 `skins/<skin-id>/` 目录：

```
skins/
  liquid-glass/
    skin.json      # 必需。{"name","author","version","targetVersion","preview"}
    skin.css       # 必需。注入的样式表本体
    preview.png    # 推荐。GUI 列表预览图
    assets/        # 可选。背景图等资源，注入时一并写入 asar
```

皮肤 CSS 编写约定：

- ZCode 桌面版基于 Tailwind + CSS 变量（`--color-background`、`--color-panel`
  等语义 token），**优先覆盖 token 而不是写死选择器**，参考
  `skin_transparent_test.css`
- 用 `!important` 兜底 Tailwind 工具类是允许的（注入的样式优先级必须压过原生的）
- 需要同时覆盖 `:root` 和 `.dark` 两个作用域（明暗两套主题）

## Commit 规范

所有提交遵循 Conventional Commits 格式：`type(scope): 主题`，主题用中文简述改动内容。
如有必要，正文用 `- xxx` 列表逐条说明改动要点。

- `type` 取值：`feat`（新功能）/ `fix`（修复）/ `docs`（文档）/ `refactor`（重构）/
  `chore`（构建、配置、依赖等杂项）
- `scope` 用括号标注影响范围，如 `feat(tauri):`、`fix(asar):`、`docs(agents):`
- 一次提交只做一件事，代码改动与文档改动分开提交
- 示例：

```
feat(tauri): 安装器支持多语言自动识别与全部用户安装

- 打包语言列表加入 SimpChinese / TradChinese，按系统语言自动匹配
- installMode 改为 both，允许用户选择当前用户或所有用户安装
```

## 注意事项

- 写入 `Program Files` 需要管理员权限；脚本已实现 UAC 自动提权，GUI 沿用该逻辑
- asar 结构改动必须重算 integrity 哈希，勿手改 asar
- `ZCode/` 目录下有官方安装文件与解包产物（`_unpack`、`asar_file_list.txt`），
  仅作分析用，不要纳入皮肤管理流程
