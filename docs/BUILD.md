# 构建与运行

## 依赖
- Node.js >= 20
- Rust >= 1.80（`cargo` 在 PATH 中）

## Windows 一键开发（两个终端）

```powershell
# 终端 1：后端（默认 0.0.0.0:8080）
cd backend
cargo run

# 终端 2：前端 dev server（5173，/api 与 /artcraft 代理到 8080）
cd frontend
npm install
npm run dev
```

打开 http://127.0.0.1:5173/office?url=https://calibre-ebook.com/downloads/demos/demo.docx

## 生产构建

```powershell
cd frontend
npm ci
npm run build          # 产物落到 backend/static

cd ..\backend
cargo build --release
.\target\release\artcraft-office.exe
```

打开 http://127.0.0.1:8080/office?url=...

## 常用脚本

| 脚本 | 作用 |
| --- | --- |
| `scripts\dev-backend.ps1` | 启动后端（可从任意目录执行） |
| `scripts\dev-frontend.ps1` | 启动前端 dev server |
| `scripts\build.ps1` | 前端 + 后端全量构建 |
