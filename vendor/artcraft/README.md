# vendor/artcraft —— 内核产物放置说明

本目录**不放二进制**，构建时也不会被打包进 git（见 `.gitignore`）。
`frontend` 的 `vite build` 不会处理这里的内容，后端把 `backend/static/artcraft/` 作为静态目录对外提供。

## 前端期望的产物

构建完成后，`backend/static/artcraft/` 下应至少有两个文件：

```
backend/static/artcraft/
├── artcraft.js        # 胶水 JS，挂载全局 window.ArtCraft
└── artcraft_bg.wasm   # WASM 主包
```

`window.ArtCraft` 需要符合下面的最小契约（`frontend/src/artcraft/loader.ts` 是唯一读取它的地方）：

```ts
window.ArtCraft = {
  WordCraft: { create(opts: CreateOptions): EditorInstance }, // docx
  GridCraft: { create(opts: CreateOptions): EditorInstance }, // xlsx
  DeckCraft: { create(opts: CreateOptions): EditorInstance }, // pptx
}

interface CreateOptions {
  container: HTMLElement
  binary: ArrayBuffer
  data: Uint8Array
  fileName: string
  readOnly: boolean
}

interface EditorInstance {
  destroy(): void
  save(): Blob | ArrayBuffer | Promise<Blob | ArrayBuffer> // 别名 export / exportAsync / toBlob 也可
  setReadOnly?(value: boolean): void
}
```

> 截至 2026-11，ArtCraft 三件套（WordCraft / GridCraft / DeckCraft）**没有发布 npm 包，也没有稳定的浏览器 SDK 入口**，
> 仓库里提供的是完整应用（如 `apps/wordcraft-web`），不是 `create()` 形态的内核 API。
> 因此本项目的策略是：**契约留在这里，产物由你按官方 release 自行编译 / 放置**；
> 产物缺失时前端自动降级为只读结构预览（不会白屏）。

## 如何生成产物

以 WordCraft 为例（其余两件套同理，仓库分别为 `storytold/gridcraft`、`storytold/deckcraft`）：

```bash
git clone https://github.com/storytold/wordcraft
cd wordcraft
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# 编译 web 目标（crate 名以仓库实际为准）
wasm-pack build --target web --release --out-dir pkg
```

拿到 `pkg/` 后：

1. 把包内 JS 重命名为 `artcraft.js`，把 `*_bg.wasm` 重命名为 `artcraft_bg.wasm`；
2. 在 `artcraft.js` 末尾追加适配层（把官方导出包装成上面的契约）：

```js
// 追加在 artcraft.js 末尾：把官方导出包装成 loader.ts 期望的契约
window.ArtCraft = window.ArtCraft || {}
window.ArtCraft.WordCraft = {
  create(opts) {
    // 按官方 API 实现：创建容器、灌入 opts.binary、设置只读
    // 返回 { destroy, save, setReadOnly }
  },
}
```

3. 复制到 `backend/static/artcraft/`（前端 dev 模式下也可放 `frontend/public/artcraft/`，vite 会原样拷贝）：
4. 访问 `/api/health`，`editor_kernel_ready` 应为 `true`。

## 许可

ArtCraft 三件套为 MIT / Apache-2.0 双许可。二次分发与 SaaS 商用前请复核仓库 LICENSE 与当时的版本条款。
