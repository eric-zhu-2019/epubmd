import { defineConfig } from 'vite';

const tauriDevHost = process.env.TAURI_DEV_HOST;

export default defineConfig({
  clearScreen: false,
  server: {
    host: tauriDevHost || '127.0.0.1',
    port: 1420,
    strictPort: true,
    hmr: tauriDevHost
      ? {
          protocol: 'ws',
          host: tauriDevHost,
          port: 1421,
        }
      : undefined,
  },
  build: {
    outDir: 'dist-web',
    emptyOutDir: true,
  },
});
