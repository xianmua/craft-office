import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import UnoCSS from 'unocss/vite'
import { fileURLToPath, URL } from 'node:url'

// 后端默认监听 127.0.0.1:8080（可用 ARTCRAFT_BIND 覆盖）
const BACKEND = process.env.ARTCRAFT_BACKEND ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [vue(), UnoCSS()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  server: {
    port: 5173,
    proxy: {
      '/api': { target: BACKEND, changeOrigin: true },
      '/artcraft': { target: BACKEND, changeOrigin: true },
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
