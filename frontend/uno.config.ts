import { defineConfig, presetUno } from 'unocss'

export default defineConfig({
  presets: [presetUno()],
  shortcuts: {
    'flex-center': 'flex items-center justify-center',
    'page-shell': 'h-screen w-screen flex flex-col overflow-hidden',
    'surface-card': 'bg-white rounded-lg border border-solid border-gray-200',
    'text-muted': 'text-gray-500',
  },
  rules: [
    ['scrollbar-thin', { 'scrollbar-width': 'thin' }],
  ],
})
