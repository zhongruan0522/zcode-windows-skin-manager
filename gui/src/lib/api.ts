import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

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

/** 导入成功的一个皮肤 */
export interface ImportedSkin {
  id: string;
  name: string;
}

/** ZIP 导入结果: 导入的皮肤列表 + 汇总消息 */
export interface ImportOutcome {
  skins: ImportedSkin[];
  message: string;
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
    // 浏览器调试: 用隐藏 input 模拟文件选择(仅能拿到文件名; 取消时不返回)
    pickSkinZip: async (): Promise<string | null> =>
      new Promise((resolve) => {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = ".zip,.ZIP";
        input.onchange = () => resolve(input.files?.[0]?.name ?? null);
        input.click();
      }),
    importSkinZip: async (path: string): Promise<ImportOutcome> => {
      await delay(600);
      const id = `zip-${Date.now()}`;
      const name = `ZIP 导入: ${path}`;
      skins.push(mk(id, name, "从压缩包导入的模拟皮肤", "#9fd3c7"));
      return {
        skins: [{ id, name }],
        message: `已导入压缩包「${path}」中的皮肤（模拟）`,
      };
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
    zcodeRunning: async (): Promise<boolean> => {
      await delay(300);
      // 浏览器调试: URL 井号带 zcode-running 时模拟 ZCode 正在运行
      return location.hash.includes("zcode-running");
    },
    launchZcode: async (): Promise<void> => {
      await delay(400);
      console.info("[mock] 启动 ZCode.exe");
    },
  };
})();

export const api = hasTauri
  ? {
      listSkins: (): Promise<SkinInfo[]> => invoke("list_skins"),
      /** 弹出系统文件选择框, 仅允许选择 .zip; 取消返回 null */
      pickSkinZip: async (): Promise<string | null> => {
        const picked = await open({
          multiple: false,
          title: "选择皮肤压缩包",
          filters: [{ name: "皮肤压缩包 (*.zip)", extensions: ["zip", "ZIP"] }],
        });
        return typeof picked === "string" ? picked : null;
      },
      importSkinZip: (path: string): Promise<ImportOutcome> =>
        invoke("import_skin_zip", { path }),
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
      zcodeRunning: (target?: string): Promise<boolean> =>
        invoke("zcode_running", { target: target ?? null }),
      launchZcode: (target?: string): Promise<void> =>
        invoke("launch_zcode", { target: target ?? null }),
    }
  : mock;
