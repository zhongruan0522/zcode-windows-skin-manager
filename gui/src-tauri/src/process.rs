//! ZCode 进程检测, 移植自 liquid_glass_skin.py 的 zcode_running_under:
//! 只检测"目标目录下"的 ZCode.exe 是否在运行（其他目录的实例不占用目标的 asar）。

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
    let target = normalize_dir(target_dir);
    stdout.lines().any(|line| {
        let line = line.trim();
        !line.is_empty() && normalize_str(&parent_of(line)).starts_with(&target)
    })
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
}
