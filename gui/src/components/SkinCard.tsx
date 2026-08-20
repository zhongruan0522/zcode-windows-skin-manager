import type { BusyAction } from "../lib/api";
import type { SkinRow } from "../views/SkinList";

interface Props {
  row: SkinRow;
  /** 该行是否为当前生效的外观 */
  inUse: boolean;
  /** 是否安装了任意第三方皮肤（决定官方行是否显示"恢复原版"） */
  installedAny: boolean;
  busy: BusyAction | null;
  onApply: (id: string) => void;
  onRestore: () => void;
  onDetail: (id: string) => void;
}

export default function SkinCard({
  row,
  inUse,
  installedAny,
  busy,
  onApply,
  onRestore,
  onDetail,
}: Props) {
  const disabled = busy !== null;
  const applyBusy = busy?.kind === "install" && busy.id === row.id;
  const restoreBusy = busy?.kind === "restore";
  // 官方行: 只要装了第三方皮肤就可"恢复原版"; 皮肤行: 该皮肤已应用时可"恢复原版"
  const showRestore = row.official ? installedAny : inUse;

  return (
    <tr className={inUse ? "row-in-use" : undefined}>
      <td className="col-preview">
        <div className="preview-box" tabIndex={0}>
          {row.previewUrl ? (
            <img src={row.previewUrl} alt={`${row.name} 预览图`} />
          ) : (
            <div className="preview-empty">暂无预览</div>
          )}
          <div className="preview-overlay">
            <button className="btn" disabled={disabled} onClick={() => onDetail(row.id)}>
              查看详情
            </button>
            {!row.official && !inUse && (
              <button
                className="btn btn-solid"
                disabled={disabled}
                onClick={() => onApply(row.id)}
              >
                {applyBusy ? "应用中…" : "应用"}
              </button>
            )}
            {showRestore && (
              <button className="btn btn-solid" disabled={disabled} onClick={onRestore}>
                {restoreBusy ? "恢复中…" : "恢复原版"}
              </button>
            )}
          </div>
        </div>
      </td>
      <td className="cell-name">
        <div className="skin-name">
          {row.name}
          {row.source === "user" && <span className="tag">用户</span>}
        </div>
        {row.description && <div className="skin-desc">{row.description}</div>}
      </td>
      <td className="col-meta">{row.author}</td>
      <td className="col-meta">{row.version}</td>
      <td className="col-status">
        {inUse ? (
          <span className="badge">使用中</span>
        ) : (
          <span className="badge badge-muted">未使用</span>
        )}
      </td>
    </tr>
  );
}
