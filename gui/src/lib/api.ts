import { invoke } from "@tauri-apps/api/core";

// 与 Rust 侧 DTO 对齐（均为 camelCase 序列化）

export interface SkinInfo {
  id: string;
  name: string;
  author: string;
  version: string;
  description: string;
  source: "builtin" | "user";
  previewDataUrl: string | null;
}

export interface StatusInfo {
  targetDir: string;
  asarExists: boolean;
  hasBackup: boolean;
  installedSkinId: string | null;
  installedSkinName: string | null;
  zcodeRunning: boolean;
  isElevated: boolean;
}

export interface Settings {
  targetDir: string;
}

/** 自动检测到的一个 ZCode 安装目录 */
export interface DetectedInstall {
  path: string;
  /** 来源: 注册表 / 常见位置 / 运行中的进程 */
  source: string;
}

export interface ActionOutcome {
  message: string;
  status: StatusInfo;
}

/** 正在执行的后台操作（应用 / 恢复） */
export interface BusyAction {
  kind: "install" | "restore";
  id: string;
}

const hasTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

// 浏览器直接打开 vite dev server（无 Tauri 后端）时使用的模拟实现，便于纯前端调试
const mock = (() => {
  let installed: string | null = null;
  const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));
  const mk = (
    id: string,
    name: string,
    description: string,
    grad: string
  ): SkinInfo => ({
    id,
    name,
    author: "zhongruan",
    version: "0.1.0",
    description,
    source: "builtin",
    previewDataUrl: `data:image/svg+xml;utf8,${encodeURIComponent(
      `<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 800 500'>` +
        `<defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'>` +
        `<stop offset='0' stop-color='${grad}'/><stop offset='1' stop-color='#ffffff'/>` +
        `</linearGradient></defs>` +
        `<rect width='800' height='500' fill='url(#g)'/>` +
        `<rect x='0' y='0' width='800' height='44' fill='rgba(255,255,255,.6)'/>` +
        `<rect x='0' y='44' width='176' height='456' fill='rgba(255,255,255,.45)'/>` +
        `<rect x='204' y='78' width='576' height='190' fill='rgba(255,255,255,.55)'/>` +
        `<rect x='204' y='296' width='576' height='156' fill='rgba(255,255,255,.55)'/></svg>`
    )}`,
  });
  const skins: SkinInfo[] = [
    mk("liquid-glass", "液态玻璃", "毛玻璃 + 极光背景的半透明皮肤（模拟数据）", "#a5c8f0"),
    mk("transparent-test", "纯透明测试", "验证注入链路的全透明皮肤（模拟数据）", "#d9d9de"),
  ];
  const status = (): StatusInfo => ({
    targetDir: "C:\\Program Files\\ZCode",
    asarExists: true,
    hasBackup: installed !== null,
    installedSkinId: installed,
    installedSkinName: installed
      ? skins.find((s) => s.id === installed)?.name ?? installed
      : null,
    zcodeRunning: false,
    isElevated: false,
  });
  return {
    listSkins: async (): Promise<SkinInfo[]> => {
      await delay(200);
      return skins;
    },
    getStatus: async (): Promise<StatusInfo> => {
      await delay(100);
      return status();
    },
    getSettings: async (): Promise<Settings> => ({
      targetDir: "C:\\Program Files\\ZCode",
    }),
    saveSettings: async (targetDir: string): Promise<Settings> => ({ targetDir }),
    detectInstalls: async (): Promise<DetectedInstall[]> => {
      await delay(400);
      return [
        { path: "C:\\Program Files\\ZCode", source: "注册表" },
        { path: "D:\\Apps\\ZCode", source: "常见位置" },
      ];
    },
    installSkin: async (id: string): Promise<ActionOutcome> => {
      await delay(900);
      installed = id;
      return {
        message: `已应用皮肤「${skins.find((s) => s.id === id)?.name ?? id}」(模拟)`,
        status: status(),
      };
    },
    restoreSkin: async (): Promise<ActionOutcome> => {
      await delay(900);
      installed = null;
      return { message: "已还原官方 app.asar (模拟)", status: status() };
    },
  };
})();

export const api = hasTauri
  ? {
      listSkins: (): Promise<SkinInfo[]> => invoke("list_skins"),
      getStatus: (target?: string): Promise<StatusInfo> =>
        invoke("get_status", { target: target ?? null }),
      getSettings: (): Promise<Settings> => invoke("get_settings"),
      saveSettings: (targetDir: string): Promise<Settings> =>
        invoke("save_settings", { targetDir }),
      detectInstalls: (): Promise<DetectedInstall[]> => invoke("detect_installs"),
      installSkin: (id: string, target?: string): Promise<ActionOutcome> =>
        invoke("install_skin", { id, target: target ?? null }),
      restoreSkin: (target?: string): Promise<ActionOutcome> =>
        invoke("restore_skin", { target: target ?? null }),
    }
  : mock;
