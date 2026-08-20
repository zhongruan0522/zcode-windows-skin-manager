import type { BusyAction, StatusInfo } from "../lib/api";

interface Props {
  status: StatusInfo | null;
  busy: BusyAction | null;
  onGoSettings: () => void;
}

export default function StatusBar({ status, busy, onGoSettings }: Props) {
  return (
    <footer className="status-bar">
      <span className="status-item">
        目标目录：
        <button className="link" onClick={onGoSettings} title="点击修改安装目录">
          {status?.targetDir ?? "…"}
        </button>
      </span>
      <span className="status-item">
        皮肤状态：
        {status == null
          ? "读取中…"
          : status.installedSkinName
            ? `已安装「${status.installedSkinName}」`
            : "未安装（官方原版）"}
      </span>
      {status?.hasBackup && <span className="status-item">备份可用，可随时还原</span>}
      {status && !status.asarExists && (
        <span className="status-item status-warn">未找到 app.asar，请检查安装目录</span>
      )}
      {status?.zcodeRunning && (
        <span className="status-item status-warn">检测到 ZCode 正在运行</span>
      )}
      {status?.isElevated && <span className="status-item">管理员模式</span>}
      {busy && (
        <span className="status-item status-busy">
          {busy.kind === "install"
            ? "正在应用皮肤（若弹出 UAC 窗口请确认）…"
            : "正在恢复原版（若弹出 UAC 窗口请确认）…"}
        </span>
      )}
    </footer>
  );
}
