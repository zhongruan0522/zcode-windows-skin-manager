import { useEffect } from "react";
import type { BusyAction, SkinInfo, StatusInfo } from "../lib/api";
import officialPreview from "../assets/official-preview.png";

interface Props {
  /** 缺省表示官方原版 */
  skin?: SkinInfo;
  status: StatusInfo | null;
  busy: BusyAction | null;
  onApply?: (id: string) => void;
  onRestore: () => void;
  onClose: () => void;
}

export default function PreviewModal({
  skin,
  status,
  busy,
  onApply,
  onRestore,
  onClose,
}: Props) {
  const installedId = status?.installedSkinId ?? null;
  const isOfficial = !skin;
  const inUse = isOfficial ? installedId === null : installedId === skin.id;
  const showRestore = isOfficial ? installedId !== null : inUse;
  const disabled = busy !== null;
  const applyBusy =
    busy?.kind === "install" && skin !== undefined && busy.id === skin.id;
  const restoreBusy = busy?.kind === "restore";

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="皮肤详情"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <h2>{isOfficial ? "官方原版" : skin!.name}</h2>
          <button className="modal-close" onClick={onClose} aria-label="关闭">
            ×
          </button>
        </div>
        <div className="modal-body">
          <div className="modal-preview">
            {!isOfficial && skin.previewDataUrl ? (
              <img src={skin.previewDataUrl} alt={`${skin.name} 预览图`} />
            ) : (
              <img src={officialPreview} alt="官方原版预览图" />
            )}
          </div>
          <div className="modal-side">
            <dl className="modal-meta">
              <dt>作者</dt>
              <dd>{isOfficial ? "ZCode 官方" : skin!.author || "未知"}</dd>
              <dt>版本</dt>
              <dd>{isOfficial ? "—" : skin!.version || "—"}</dd>
              <dt>来源</dt>
              <dd>{isOfficial || skin!.source === "builtin" ? "内置" : "用户皮肤目录"}</dd>
              <dt>状态</dt>
              <dd>{inUse ? "使用中" : "未使用"}</dd>
            </dl>
            <p className="modal-desc">
              {isOfficial
                ? "ZCode 桌面版默认外观。应用任何皮肤前会自动备份官方 app.asar，随时可以一键还原。"
                : skin!.description || "（无描述）"}
            </p>
            <div className="modal-actions">
              {!isOfficial && !inUse && onApply && (
                <button
                  className="btn btn-solid"
                  disabled={disabled}
                  onClick={() => onApply(skin!.id)}
                >
                  {applyBusy ? "应用中…" : "应用此皮肤"}
                </button>
              )}
              {showRestore && (
                <button className="btn" disabled={disabled} onClick={onRestore}>
                  {restoreBusy ? "恢复中…" : "恢复原版"}
                </button>
              )}
            </div>
            <p className="modal-hint">
              应用或恢复前请完全退出 ZCode 桌面版；写入 Program Files
              需要管理员权限，弹出 UAC 窗口时请点“是”。
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
