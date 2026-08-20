import { useState } from "react";
import { api, type DetectedInstall } from "../lib/api";

interface Props {
  targetDir: string;
  onSaved: (dir: string) => void;
}

export default function SettingsView({ targetDir, onSaved }: Props) {
  const [value, setValue] = useState(targetDir);
  const [saving, setSaving] = useState(false);
  const [hint, setHint] = useState<string | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [candidates, setCandidates] = useState<DetectedInstall[] | null>(null);

  const save = async () => {
    setSaving(true);
    setHint(null);
    try {
      const s = await api.saveSettings(value.trim());
      onSaved(s.targetDir);
      setHint("已保存");
    } catch (e) {
      setHint(`保存失败：${String(e)}`);
    } finally {
      setSaving(false);
    }
  };

  const detect = async () => {
    setDetecting(true);
    setHint(null);
    setCandidates(null);
    try {
      const found = await api.detectInstalls();
      if (found.length === 0) {
        setHint("未检测到 ZCode 安装目录，请手动填写安装路径。");
      } else {
        setCandidates(found);
        setValue(found[0].path);
      }
    } catch (e) {
      setHint(`检测失败：${String(e)}`);
    } finally {
      setDetecting(false);
    }
  };

  return (
    <div className="settings">
      <h2>设置</h2>

      <section className="settings-item">
        <label htmlFor="target-dir">ZCode 安装目录</label>
        <div className="settings-row">
          <input
            id="target-dir"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="C:\Program Files\ZCode"
            spellCheck={false}
          />
          <button
            className="btn"
            disabled={detecting}
            onClick={() => void detect()}
          >
            {detecting ? "检测中…" : "自动检测"}
          </button>
          <button
            className="btn btn-solid"
            disabled={saving || value.trim() === ""}
            onClick={() => void save()}
          >
            {saving ? "保存中…" : "保存"}
          </button>
        </div>
        {candidates && candidates.length > 0 && (
          <div className="detect-list">
            <p className="settings-hint">
              检测到 {candidates.length} 个安装，点击选用后保存：
            </p>
            {candidates.map((c) => (
              <button
                key={c.path}
                type="button"
                className={
                  value.trim().toLowerCase() === c.path.toLowerCase()
                    ? "detect-item active"
                    : "detect-item"
                }
                onClick={() => setValue(c.path)}
              >
                <span className="detect-path">{c.path}</span>
                <span className="detect-src">{c.source}</span>
              </button>
            ))}
          </div>
        )}
        <p className="settings-hint">
          该目录下需存在 resources\app.asar。写入 Program Files
          需要管理员权限，应用皮肤时会自动弹出 UAC 提权窗口。
        </p>
        {hint && <p className="settings-hint">{hint}</p>}
      </section>

      <section className="settings-item">
        <h3>皮肤目录</h3>
        <p className="settings-hint">
          内置皮肤随应用提供；自定义皮肤放入{" "}
          <code>%APPDATA%\zcode-skin-manager\skins\&lt;皮肤id&gt;\</code>
          （须包含 skin.json 与 skin.css），与内置皮肤同名时优先使用用户目录的版本。
        </p>
      </section>

      <section className="settings-item">
        <h3>说明</h3>
        <ul className="settings-hint">
          <li>应用或恢复前请完全退出 ZCode 桌面版，否则会写入失败。</li>
          <li>ZCode 自动更新后皮肤会被新包覆盖，重新应用一次即可。</li>
        </ul>
      </section>
    </div>
  );
}
