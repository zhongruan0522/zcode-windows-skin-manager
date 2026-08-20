import { useCallback, useEffect, useState } from "react";
import { api, type BusyAction, type SkinInfo, type StatusInfo } from "./lib/api";
import SkinList from "./views/SkinList";
import SettingsView from "./views/Settings";
import StatusBar from "./components/StatusBar";
import PreviewModal from "./components/PreviewModal";
import ZCodeRunningDialog from "./components/ZCodeRunningDialog";
import LaunchZCodeDialog from "./components/LaunchZCodeDialog";

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
  /** 待确认的操作(应用/恢复): ZCode 运行中, 等用户退出后在弹窗里确认 */
  const [confirmPending, setConfirmPending] = useState<BusyAction | null>(null);
  /** 弹窗内点「已关闭」后的复查状态 */
  const [confirmChecking, setConfirmChecking] = useState(false);
  /** 复查后仍检测到 ZCode 在运行 */
  const [stillRunning, setStillRunning] = useState(false);
  /** 应用/还原成功后待确认的「立即打开 ZCode」消息 */
  const [launchPending, setLaunchPending] = useState<string | null>(null);

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

  const runAction = useCallback(async (action: BusyAction) => {
    setBusy(action);
    setError(null);
    setMessage(null);
    try {
      const out =
        action.kind === "install"
          ? await api.installSkin(action.id)
          : await api.restoreSkin();
      setStatus(out.status);
      // 成功后不再直接出横幅, 弹窗询问是否立即打开 ZCode
      setLaunchPending(out.message);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  }, []);

  /** 应用/恢复前的守卫: 实时检测 ZCode 是否在运行, 运行中弹窗让用户退出后确认 */
  const guardAction = useCallback(
    async (action: BusyAction) => {
      setBusy(action);
      setError(null);
      setMessage(null);
      try {
        if (await api.zcodeRunning()) {
          setStillRunning(false);
          setConfirmPending(action);
          return; // busy 保持, 弹窗期间锁定界面
        }
        await runAction(action);
      } catch (e) {
        setBusy(null);
        setError(String(e));
      }
    },
    [runAction]
  );

  const handleApply = useCallback(
    (id: string) => {
      void guardAction({ kind: "install", id });
    },
    [guardAction]
  );

  const handleRestore = useCallback(() => {
    void guardAction({ kind: "restore", id: "official" });
  }, [guardAction]);

  /** 确认弹窗「取消」: 放弃本次操作 */
  const handleConfirmCancel = useCallback(() => {
    setConfirmPending(null);
    setStillRunning(false);
    setBusy(null);
  }, []);

  /** 确认弹窗「已关闭」: 复查进程, 确已退出则继续执行原操作 */
  const handleConfirmClosed = useCallback(() => {
    if (!confirmPending) return;
    const action = confirmPending;
    setConfirmChecking(true);
    void (async () => {
      try {
        if (await api.zcodeRunning()) {
          setStillRunning(true);
          return;
        }
        setConfirmPending(null);
        setStillRunning(false);
        await runAction(action);
      } catch (e) {
        setError(String(e));
        setConfirmPending(null);
        setBusy(null);
      } finally {
        setConfirmChecking(false);
      }
    })();
  }, [confirmPending, runAction]);

  const handleSaveSettings = useCallback(async (dir: string) => {
    setTargetDir(dir);
    try {
      setStatus(await api.getStatus());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  /** 成功弹窗「取消」: 不打开 ZCode, 成功消息回落到横幅展示 */
  const handleLaunchCancel = useCallback(() => {
    setMessage(launchPending);
    setLaunchPending(null);
  }, [launchPending]);

  /** 成功弹窗「打开」或倒计时结束: 启动 ZCode 桌面版 */
  const handleLaunchZcode = useCallback(async () => {
    const msg = launchPending ?? "";
    setLaunchPending(null);
    try {
      await api.launchZcode();
      setMessage(`${msg}已启动 ZCode 桌面版。`);
    } catch (e) {
      setError(String(e));
    }
  }, [launchPending]);

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

      {confirmPending && (
        <ZCodeRunningDialog
          actionLabel={
            confirmPending.kind === "install"
              ? skins.find((s) => s.id === confirmPending.id)?.name ?? confirmPending.id
              : "恢复原版"
          }
          stillRunning={stillRunning}
          checking={confirmChecking}
          onCancel={handleConfirmCancel}
          onConfirmClosed={handleConfirmClosed}
        />
      )}

      {launchPending && (
        <LaunchZCodeDialog
          message={launchPending}
          onCancel={handleLaunchCancel}
          onLaunch={handleLaunchZcode}
        />
      )}
    </div>
  );
}
