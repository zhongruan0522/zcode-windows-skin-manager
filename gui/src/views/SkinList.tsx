import type { BusyAction, SkinInfo, StatusInfo } from "../lib/api";
import SkinCard from "../components/SkinCard";
import officialPreview from "../assets/official-preview.png";

/** 表格一行的数据（官方占位行 official=true，或第三方皮肤行） */
export interface SkinRow {
  id: string;
  name: string;
  author: string;
  version: string;
  description: string;
  previewUrl: string | null;
  official: boolean;
  source: SkinInfo["source"] | null;
}

interface Props {
  skins: SkinInfo[];
  status: StatusInfo | null;
  busy: BusyAction | null;
  onApply: (id: string) => void;
  onRestore: () => void;
  onDetail: (id: string) => void;
}

export default function SkinList({
  skins,
  status,
  busy,
  onApply,
  onRestore,
  onDetail,
}: Props) {
  const installedId = status?.installedSkinId ?? null;
  const installedAny = installedId !== null;

  // 官方原版固定第一行占位，其后是第三方皮肤
  const rows: SkinRow[] = [
    {
      id: "official",
      name: "官方原版",
      author: "ZCode 官方",
      version: "—",
      description: "ZCode 桌面版默认外观，未做任何修改",
      previewUrl: officialPreview,
      official: true,
      source: null,
    },
    ...skins.map((s): SkinRow => ({
      id: s.id,
      name: s.name,
      author: s.author || "未知作者",
      version: s.version || "—",
      description: s.description,
      previewUrl: s.previewDataUrl,
      official: false,
      source: s.source,
    })),
  ];

  return (
    <table className="skin-table">
      <thead>
        <tr>
          <th className="col-preview">预览</th>
          <th>皮肤</th>
          <th className="col-meta">作者</th>
          <th className="col-meta">版本</th>
          <th className="col-status">状态</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <SkinCard
            key={row.id}
            row={row}
            inUse={row.official ? !installedAny : installedId === row.id}
            installedAny={installedAny}
            busy={busy}
            onApply={onApply}
            onRestore={onRestore}
            onDetail={onDetail}
          />
        ))}
      </tbody>
    </table>
  );
}
