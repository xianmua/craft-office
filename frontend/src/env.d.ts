/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<{}, {}, any>
  export default component
}

interface ArtCraftSdk {
  WordCraft?: { create: (opts: unknown) => unknown }
  GridCraft?: { create: (opts: unknown) => unknown }
  DeckCraft?: { create: (opts: unknown) => unknown }
}

interface Window {
  ArtCraft?: ArtCraftSdk
}
