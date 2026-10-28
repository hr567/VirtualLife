import { defineConfig } from '@playwright/test';
import os from 'node:os';
import path from 'node:path';

export default defineConfig({
  testDir: './tests/web',
  outputDir: process.env.PLAYWRIGHT_OUTPUT_DIR ?? path.join(os.tmpdir(), 'virtual-life-web-qa'),
  timeout: 60_000,
  expect: { timeout: 20_000 },
  fullyParallel: false,
  workers: 1,
  reporter: 'list',
  use: {
    baseURL: 'http://127.0.0.1:4173/VirtualLife/',
    viewport: { width: 1100, height: 820 },
    channel: process.env.PLAYWRIGHT_CHANNEL,
    // Deterministic software WebGPU is for CI only; production uses the browser's real adapter.
    launchOptions: { args: process.env.PLAYWRIGHT_GPU === 'hardware' ? [] : process.platform === 'linux'
      ? ['--enable-unsafe-webgpu', '--use-angle=vulkan', '--enable-features=Vulkan', '--use-vulkan=swiftshader', '--use-webgpu-adapter=swiftshader', '--disable-vulkan-surface', '--enable-dawn-features=allow_unsafe_apis']
      : ['--enable-unsafe-webgpu', '--use-webgpu-adapter=swiftshader', '--enable-dawn-features=allow_unsafe_apis'] },
    screenshot: 'only-on-failure',
  },
  webServer: process.env.PLAYWRIGHT_EXTERNAL_SERVER ? undefined : {
    command: 'pnpm preview --port 4173',
    url: 'http://127.0.0.1:4173/VirtualLife/',
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
