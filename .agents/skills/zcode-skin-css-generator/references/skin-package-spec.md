# ZCode 皮肤包规范

本项目要求所有皮肤都放在 `skins/<skin-name>/` 目录下。

标准结构：

```text
skins/
  liquid-glass/
    skin.json
    skin.css
    preview.png
    assets/
```

各文件含义：

- `skin.json`：皮肤元数据文件。
- `skin.css`：实际注入到 ZCode 内部的样式表。
- `preview.png`：GUI 列表里展示的预览图，推荐提供。
- `assets/`：可选资源目录，用于存放图片、字体等附加文件。

当前 Rust 加载器的元数据行为：

- 真正硬性要求的元数据字段只有 `name`。
- `author`、`version`、`description` 在代码层面是可选的，但正常情况下应当填写。
- `preview` 是可选字段；如果写了，就应该指向真实存在的文件。
- GUI 侧除了读取 `preview` 字段，也会自动尝试 `preview.png`、`preview.jpg`、`preview.jpeg`、`preview.webp`、`preview.gif`。

本仓库里的 CSS 约定：

- 优先覆盖语义化 token，不要先写脆弱的选择器。
- 必须同时覆盖 `:root` 和 `.dark`。
- 覆盖内置 Tailwind 背景工具类时可以使用 `!important`。
- 需要兼顾明暗主题下的可读性。
- 如果依赖额外图片或其他资源，把它们放在 `assets/` 下，并使用相对路径引用。

zip 打包约定：

- 打包整个 `skins/<skin-name>/` 目录。
- 压缩包内部必须保留 `<skin-name>/` 作为顶层目录。
- 默认输出路径为 `skins/<skin-name>.zip`。
