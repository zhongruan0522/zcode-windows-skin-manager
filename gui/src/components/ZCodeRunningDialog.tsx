import { useEffect } from "react";

interface Props {
  /** 待执行操作的名称（皮肤名或「恢复原版」），用于文案提示 */
  actionLabel: string;
  /** 复查后仍检测到 ZCode 在运行 */
  stillRunning: boolean;
  /** 点击「已关闭」后正在复查进程 */
  checking: boolean;
  onCancel: () => void;
  onConfirmClosed: () => void;
}

/** ZCode 运行中确认弹窗: 提示用户保存工作并完全退出 ZCode 后再继续 */
export default function ZCodeRunningDialog({
  actionLabel,
  stillRunning,
  checking,
  onCancel,
  onConfirmClosed,
}: Props) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="modal-mask dialog-mask" onClick={onCancel}>
      <div
        className="modal dialog"
        role="alertdialog"
        aria-modal="true"
        aria-label="ZCode 正在运行"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h2>ZCode 正在运行</h2>
        </div>
        <div className="dialog-body">
          <p>
            检测到 ZCode 桌面版正在运行。请先保存工作内容并完全退出 ZCode
            （注意系统托盘里的后台图标），然后点击「已关闭」继续「{actionLabel}」。
          </p>
          {stillRunning && (
            <p className="dialog-warn">
              仍检测到 ZCode 在运行，请确认已完全退出后重试。
            </p>
          )}
        </div>
        <div className="dialog-foot">
          <button className="btn" disabled={checking} onClick={onCancel}>
            取消
          </button>
          <button className="btn btn-solid" disabled={checking} onClick={onConfirmClosed}>
            {checking ? "检测中…" : "已关闭"}
          </button>
        </div>
      </div>
    </div>
  );
}
