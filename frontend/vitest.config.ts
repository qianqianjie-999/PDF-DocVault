import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'

// 单元测试专用配置(优先于 vite.config.ts),只测纯 TS 模块,无需 vue 插件
export default defineConfig({
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.{test,spec}.ts'],
  },
})
