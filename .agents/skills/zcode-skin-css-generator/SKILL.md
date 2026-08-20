---
name: zcode-skin-css-generator
description: 生成完整的 ZCode 换肤包，而不只是零散 CSS。凡是用户要求创建、重写、美化、导出 ZCode 桌面版皮肤时，都应使用本 skill，并产出 `skin.css`、`skin.json`、预览图资源以及可导入的 zip 压缩包。
---

# ZCode 皮肤生成 Skill

这个 skill 用于在磁盘上生成完整的 ZCode 皮肤包。除非用户明确只要思路或示例，否则不要停留在输出一段 CSS。

## 必须产出的目录结构

始终创建或更新如下结构：

```text
skins/
  <skin-name>/
    skin.json
    skin.css
    preview.png
    assets/
```

要求：

- `skin-name` 必须使用小写 kebab-case。
- `skin.json` 和 `skin.css` 是必需文件。
- `preview.png` 是推荐文件；如果当前环境无法生成预览图，就不要在 `skin.json` 里留下失效的 `preview` 字段。
- `assets/` 是可选目录；当皮肤依赖额外图片、字体或其他资源时必须创建。
- 皮肤目录完成后，必须再打包出一个 zip，且 zip 内顶层必须保留 `<skin-id>/` 目录，不能把文件直接平铺到压缩包根目录。

## 固定工作流

1. 根据用户需求确定皮肤的 `skin-id` 和视觉方向。
2. 创建或更新 `skins/<skin-id>/skin.json`。
3. 创建或更新 `skins/<skin-id>/skin.css`。
4. 条件允许时补上 `preview.png`。
5. 如果 CSS 引用了额外资源，则补上 `assets/` 目录及对应文件。
6. 把 `skins/<skin-id>/` 打包成 zip。

如果用户要的是“实际皮肤”，就直接写文件，不要只返回说明文字。

## `skin.json` 约定

优先使用下面这个结构（当前值为示例值）：

```json
{
  "name": "液态玻璃",
  "author": "zhongruan",
  "version": "0.1.0",
  "targetVersion": "ZCode 桌面版（Electron）",
  "preview": "preview.png",
  "description": "毛玻璃 + 极光背景的半透明皮肤, 同时适配明暗两套主题"
}
```

填写原则：

- `name` 是给用户看的显示名，要可读。
- `version` 尽量使用语义化版本。
- `targetVersion` 默认描述为 ZCode 桌面版目标，除非用户明确要求更具体的目标版本。
- `description` 保持简短、具体。
- 只有预览图真的存在时，才写入 `preview` 字段。

## 本项目里的 CSS 编写规则

这里不是通用网页换肤，必须遵守这个仓库的约定。

- ZCode 主要通过语义化 CSS 变量换肤，例如 `--color-background`、`--color-panel`、`--color-card`、`--color-popover`、`--color-input` 以及相关边框、悬浮态 token。
- 优先覆盖 token，不够时再补直接选择器。
- 必须同时覆盖 `:root` 和 `.dark`，保证明亮与暗色两套主题都可用。
- 为了压过内置 Tailwind 工具类，允许使用 `!important`。
- 如果加了模糊、半透明或玻璃效果，也要同步处理可读性、边框、选中文本颜色、滚动条等细节。
- 如果皮肤依赖 `assets/` 里的文件，`skin.css` 中必须使用相对路径引用。

## 打包要求

需要打包成zip格式压缩包，且压缩包内文件架构符合下列

```
skins/
  {skins-name}/
    skin.json      # 必需。{"name","author","version","targetVersion","preview"}
    skin.css       # 必需。注入的样式表本体
    preview.png    # 推荐。GUI 列表预览图
    assets/        # 可选。背景图等资源，注入时一并写入 asar
```

## 完成前自检

结束前自行核对：

- `skins/<skin-name>/skin.json` 已存在且是合法 JSON。
- `skins/<skin-name>/skin.css` 已存在，且视觉方向符合用户要求。
- 如果 `skin.json` 声明了 `preview`，对应文件确实存在。
- CSS 引用到的资源文件都真实存在于 `assets/` 下。
- zip 压缩包已生成，且内部包含完整皮肤目录。

## 参考文档

如果需要快速回忆这个项目的皮肤包格式和元数据行为，先读取 `references/skin-package-spec.md`。
