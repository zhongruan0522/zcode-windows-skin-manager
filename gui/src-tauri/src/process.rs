//! ZCode 进程检测与启动。检测部分移植自 liquid_glass_skin.py 的
//! zcode_running_under: 只检测"目标目录下"的 ZCode.exe 是否在运行
//! （其他目录的实例不占用目标的 asar）。

use std::path::Path;
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// CREATE_NO_WINDOW: 子进程不分配新的控制台窗口, 避免 PowerShell 一闪而过
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 构造一个不弹黑窗的 PowerShell 调用, 输出运行中的 ZCode.exe 路径。
/// process.rs / paths.rs 共用同一查询, 避免命令字符串重复散落。
pub(crate) fn powershell_zcode_paths() -> Command {
    let mut cmd = Command::new("powershell");
    cmd.args([
        "-NoProfile",
        "-Command",
        "(Get-Process ZCode -ErrorAction SilentlyContinue).Path",
    ]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn zcode_running_under(target_dir: &Path) -> bool {
    let Ok(out) = powershell_zcode_paths().output() else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    stdout
        .lines()
        .any(|line| exe_path_matches_target(line.trim(), target_dir))
}

/// PowerShell 输出的一行进程 exe 路径是否位于目标目录下。
/// ZCode.exe 标准布局就安装在目标目录**根部**, exe 父目录与目标目录
/// 完全相等, 必须判等命中; 只比 `dir\` 前缀会漏掉这种最常见的情况
/// (曾经的 bug: 检测永远不命中, 导致 ZCode 运行中照样写回 asar)。
fn exe_path_matches_target(exe_path: &str, target_dir: &Path) -> bool {
    if exe_path.is_empty() {
        return false;
    }
    let dir = normalize_str(&parent_of(exe_path));
    let target = normalize_dir(target_dir); // 尾部带 '\'
    let target_root = target.trim_end_matches('\\');
    dir == target_root || dir.starts_with(&target)
}

fn parent_of(path: &str) -> String {
    let p = Path::new(path);
    p.parent().unwrap_or(p).to_string_lossy().into_owned()
}

fn normalize_dir(dir: &Path) -> String {
    let mut s = normalize_str(&dir.to_string_lossy());
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s
}

pub(crate) fn normalize_str(p: &str) -> String {
    let lowered = p.to_lowercase().replace('/', "\\");
    // canonicalize 会引入 \\?\ 前缀, 去掉后再比较
    match std::fs::canonicalize(&lowered) {
        Ok(can) => strip_verbatim(can.to_string_lossy().to_lowercase().replace('/', "\\")),
        Err(_) => strip_verbatim(lowered),
    }
}

fn strip_verbatim(s: String) -> String {
    s.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(s)
}

/// 启动目标目录下的 ZCode.exe(分离进程, 不等待其退出)。
/// 本应用以管理员身份运行时, 拉起的 ZCode 会继承管理员权限——
/// 写入 Program Files 的安装目录本就要求管理员, 属预期行为。
pub fn launch_zcode(target_dir: &Path) -> Result<(), String> {
    let exe = target_dir.join("ZCode.exe");
    if !exe.is_file() {
        return Err(format!(
            "找不到 {}, 请在「设置」中确认 ZCode 安装目录。",
            exe.display()
        ));
    }
    Command::new(&exe)
        .current_dir(target_dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("启动 ZCode 失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_running_for_empty_temp_dir() {
        let dir = crate::asar::temp_root("process-none");
        // 该目录下没有 ZCode.exe, 必然不算运行
        assert!(!zcode_running_under(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exe_path_matches_target_root_subdir_and_rejects_other_dir() {
        let dir = crate::asar::temp_root("process-match");
        let exe = dir.join("ZCode.exe");
        std::fs::write(&exe, b"x").unwrap();
        // 回归: 标准布局 exe 直接位于目标目录根部, 必须命中
        // (旧实现只比 "dir\" 前缀, 相等目录永远不命中)
        assert!(exe_path_matches_target(&exe.to_string_lossy(), &dir));
        // 子目录里的实例同样算目标目录下的进程
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let sub = dir.join("sub").join("ZCode.exe");
        std::fs::write(&sub, b"x").unwrap();
        assert!(exe_path_matches_target(&sub.to_string_lossy(), &dir));
        // 其他目录的实例不算
        let other = crate::asar::temp_root("process-other");
        let other_exe = other.join("ZCode.exe");
        std::fs::write(&other_exe, b"x").unwrap();
        assert!(!exe_path_matches_target(&other_exe.to_string_lossy(), &dir));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&other);
    }
}
