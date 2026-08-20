import { useCallback, useEffect, useState } from "react";
import { api, type BusyAction, type SkinInfo, type StatusInfo } from "./lib/api";
import SkinList from "./views/SkinList";
import SettingsView from "./views/Settings";
import StatusBar from "./components/StatusBar";
import PreviewModal from "./components/PreviewModal";

type View = "skins" | "settings";

export default function App() {
  const [view, setView] = useState<View>("skins");
  const [skins, setSkins] = useState<SkinInfo[]>([]);
  const [status, setStatus] = useState<StatusInfo | null>(null);
  const [targetDir, setTargetDir] = useState("");
  const [busy, setBusy] = useState<BusyAction | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** 详情弹窗: "official" 或皮肤 id */
  const [detailId, setDetailId] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const settings = await api.getSettings();
        setTargetDir(settings.targetDir);
        setStatus(await api.getStatus());
        setSkins(await api.listSkins());
      } catch (e) {
        setError(String(e));
      }
    })();
  }, []);

  const runAction = useCallback(
    async (action: BusyAction, fn: () => Promise<void>) => {
      setBusy(action);
      setError(null);
      setMessage(null);
      try {
        await fn();
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
    },
    []
  );

  const handleApply = useCallback(
    (id: string) => {
      void runAction({ kind: "install", id }, async () => {
        const out = await api.installSkin(id);
        setMessage(out.message);
        setStatus(out.status);
      });
    },
    [runAction]
  );

  const handleRestore = useCallback(() => {
    void runAction({ kind: "restore", id: "official" }, async () => {
      const out = await api.restoreSkin();
      setMessage(out.message);
      setStatus(out.status);
    });
  }, [runAction]);

  const handleSaveSettings = useCallback(async (dir: string) => {
    setTargetDir(dir);
    try {
      setStatus(await api.getStatus());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const detailSkin =
    detailId && detailId !== "official"
      ? skins.find((s) => s.id === detailId) ?? null
      : null;

  return (
    <div className="app">
      <header className="app-header">
        <h1>ZCode皮肤管理器</h1>
        <nav className="tabs">
          <button
            className={view === "skins" ? "tab active" : "tab"}
            onClick={() => setView("skins")}
          >
            皮肤
          </button>
          <button
            className={view === "settings" ? "tab active" : "tab"}
            onClick={() => setView("settings")}
          >
            设置
          </button>
        </nav>
      </header>

      <main className="app-main">
        {error && (
          <div className="banner banner-error">
            <span>{error}</span>
            <button className="banner-close" onClick={() => setError(null)} aria-label="关闭提示">
              ×
            </button>
          </div>
        )}
        {!error && message && (
          <div className="banner">
            <span>{message}</span>
            <button className="banner-close" onClick={() => setMessage(null)} aria-label="关闭提示">
              ×
            </button>
          </div>
        )}

        {view === "skins" ? (
          <SkinList
            skins={skins}
            status={status}
            busy={busy}
            onApply={handleApply}
            onRestore={handleRestore}
            onDetail={setDetailId}
          />
        ) : (
          <SettingsView targetDir={targetDir} onSaved={handleSaveSettings} />
        )}
      </main>

      <StatusBar status={status} busy={busy} onGoSettings={() => setView("settings")} />

      {detailId === "official" && (
        <PreviewModal
          status={status}
          busy={busy}
          onRestore={handleRestore}
          onClose={() => setDetailId(null)}
        />
      )}
      {detailSkin && (
        <PreviewModal
          skin={detailSkin}
          status={status}
          busy={busy}
          onApply={handleApply}
          onRestore={handleRestore}
          onClose={() => setDetailId(null)}
        />
      )}
    </div>
  );
}
