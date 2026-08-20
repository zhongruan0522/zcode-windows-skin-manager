import { useEffect, useRef, useState } from "react";

/** 自动打开的倒计时秒数 */
const COUNTDOWN = 5;

interface Props {
  /** 操作成功消息（如「已应用皮肤「液态玻璃」。」） */
  message: string;
  onCancel: () => void;
  onLaunch: () => void;
}

/** 应用/还原成功后的弹窗: 询问是否立即打开 ZCode。
 *  右下角「打开」按钮带倒计时, 计时结束仍未取消则自动打开。 */
export default function LaunchZCodeDialog({ message, onCancel, onLaunch }: Props) {
  const [left, setLeft] = useState(COUNTDOWN);

  // 用 ref 保存回调, 避免其身份变化导致倒计时被重置
  const onLaunchRef = useRef(onLaunch);
  useEffect(() => {
    onLaunchRef.current = onLaunch;
  }, [onLaunch]);

  // Escape 视为取消
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  // 倒计时: 每秒 -1, 归零自动打开
  useEffect(() => {
    if (left <= 0) {
      onLaunchRef.current();
      return;
    }
    const id = setTimeout(() => setLeft((n) => n - 1), 1000);
    return () => clearTimeout(id);
  }, [left]);

  return (
    <div className="modal-mask dialog-mask" onClick={onCancel}>
      <div
        className="modal dialog"
        role="alertdialog"
        aria-modal="true"
        aria-label="打开 ZCode"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h2>操作完成</h2>
        </div>
        <div className="dialog-body">
          <p>{message}</p>
          <p className="dialog-hint">
            是否立即打开 ZCode 查看效果？
            {left > 0 ? ` ${left} 秒后将自动打开。` : " 正在打开…"}
          </p>
        </div>
        <div className="dialog-foot">
          <button className="btn" onClick={onCancel}>
            取消
          </button>
          <button className="btn btn-solid" onClick={onLaunch}>
            {left > 0 ? `打开 (${left}s)` : "打开…"}
          </button>
        </div>
      </div>
    </div>
  );
}
