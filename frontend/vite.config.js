import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import AutoImport from 'unplugin-auto-import/vite';
import Components from 'unplugin-vue-components/vite';
import { ElementPlusResolver } from 'unplugin-vue-components/resolvers';
import { fileURLToPath, URL } from 'node:url';
// Tauri 桌面端专用 Vite 配置
var host = process.env.TAURI_DEV_HOST;
export default defineConfig(function () { return ({
    plugins: [
        vue(),
        AutoImport({
            imports: ['vue', 'vue-router', 'pinia'],
            resolvers: [ElementPlusResolver({ importStyle: 'sass' })],
            dts: 'src/auto-imports.d.ts',
        }),
        Components({
            resolvers: [ElementPlusResolver({ importStyle: 'sass' })],
            dts: 'src/components.d.ts',
        }),
    ],
    resolve: {
        alias: {
            '@': fileURLToPath(new URL('./src', import.meta.url)),
        },
    },
    css: {
        preprocessorOptions: {
            scss: {
                additionalData: "@use \"@/styles/variables.scss\" as *;",
            },
        },
    },
    // Tauri 要求固定端口 + 清屏
    clearScreen: false,
    server: {
        port: 5173,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                protocol: 'ws',
                host: host,
                port: 5174,
            }
            : undefined,
        watch: {
            ignored: ['**/src-tauri/**'],
        },
    },
    envPrefix: ['VITE_', 'TAURI_ENV_*'],
    build: {
        target: ['es2021', 'chrome100', 'safari13'],
        minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
        sourcemap: !!process.env.TAURI_ENV_DEBUG,
    },
}); });
