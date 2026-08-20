# ZCode 皮肤管理器 GUI (Tauri)

构建命令（在 `gui/` 下执行）：

- `pnpm install` — 安装前端依赖
- `pnpm tauri dev` — 开发调试（首次需编译 Rust，较慢；窗口标题“ZCode皮肤管理器”）
- `pnpm tauri build` — 打包 NSIS 安装器（`../../skins` 经 tauri.conf.json 的 resources 随包分发）
- `pnpm dev` — 仅启动前端，浏览器打开 http://localhost:1420，无 Tauri 后端时
  `src/lib/api.ts` 自动切换为 mock 实现，便于纯前端调样式
- `pnpm build` — 前端 tsc 类型检查 + 产出 `dist/`
- `cargo test`（在 `src-tauri/` 下）— 后端测试：asar 读写/integrity、安装-换肤-还原全流程、
  皮肤包扫描。测试用合成 asar，不触碰真实安装目录

## 结构与约定

- `src-tauri/src/` 各模块与根目录 `liquid_glass_skin.py` 的功能块一一对应
  （asar / inject / skins / elevate / process / paths），移植时保持逻辑等价；
  `tray.rs` 为 GUI 附加模块（Python 版没有对应物）
- 系统托盘（`tray.rs`）：关闭主窗口只隐藏到托盘继续驻留，托盘菜单"退出"才真正退出；
  左键托盘图标唤起主窗口，右键菜单含 GitHub / 建议反馈（ShellExecuteW 开浏览器，
  链接写死在 tray.rs，改仓库地址时同步改）
- Rust DTO 一律 `#[serde(rename_all = "camelCase")]`，与 `src/lib/api.ts` 的 TS 类型对齐；
  新增字段两边同步改
- 后端读皮肤目录：开发时经 `paths::builtin_skins_dir()` 从 exe 向上找仓库根 `skins/`，
  打包后读 exe 旁 `skins/`；用户皮肤目录固定为 `%APPDATA%\zcode-skin-manager\skins`
- 提权方式：启动时若非管理员，`elevate::relaunch_as_admin_if_needed` 以
  `runas` 自动提权重启自身（双击桌面图标即弹 UAC，无需用户手动"以管理员身份运行"）；
  写入仍被拒时以 `--elevated` 参数后台执行，子进程结果经临时 JSON 回传
  （见 `elevate.rs`），不要改成 shell 调 Python
- 命令均为 `#[tauri::command] async fn`，阻塞操作放
  `tauri::async_runtime::spawn_blocking`，避免卡住主线程
