import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import { fileURLToPath, URL } from 'node:url'

// 后端监听地址见 backend/config.toml（当前 0.0.0.0:7778，可用 ARTCRAFT_BACKEND 覆盖）
const BACKEND = process.env.ARTCRAFT_BACKEND ?? 'http://127.0.0.1:7778'

// 需转发到后端的路径前缀：api/外壳 + 三内核静态目录（dev 下避免被 SPA 回退劫持）
const PREFIXES = ['/api', '/fonts', '/wordcraft', '/gridcraft', '/deckcraft']

// 子路径部署前缀（如 ARTCRAFT_BASE=/office/），默认根路径；前端代码通过 import.meta.env.BASE_URL 感知
const RAW_BASE = (process.env.ARTCRAFT_BASE ?? '/').trim()
const BASE = !RAW_BASE || RAW_BASE === '/' ? '/' : `/${RAW_BASE.replace(/^\/+|\/+$/g, '')}/`

export default defineConfig({
  base: BASE,
  plugins: [vue(), UnoCSS()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 7777,
    proxy: {
      // 根路径：前缀直接转发
      ...Object.fromEntries(PREFIXES.map((p) => [p, { target: BACKEND, changeOrigin: true }])),
      // 子路径 base（如 /office/）下请求带前缀，剥前缀后转发到后端
      ...(BASE === '/'
        ? {}
        : Object.fromEntries(
            PREFIXES.map((p) => [
              `${BASE}${p.slice(1)}`,
              {
                target: BACKEND,
                changeOrigin: true,
                rewrite: (path: string) => path.slice(BASE.length - 1),
              },
            ]),
          )),
    },
  },
  build: {
    outDir: '../backend/static',
    // 不能清空：wordcraft/（完整编辑器 wasm 产物）由部署脚本单独拷入，会被误删
    emptyOutDir: false,
    assetsDir: 'assets',
    target: 'es2020',
  },
})
