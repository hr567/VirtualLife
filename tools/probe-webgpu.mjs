import { chromium } from '@playwright/test';
const args = process.platform === 'linux'
  ? ['--enable-unsafe-webgpu', '--use-angle=vulkan', '--enable-features=Vulkan', '--use-vulkan=swiftshader', '--use-webgpu-adapter=swiftshader', '--disable-vulkan-surface', '--enable-dawn-features=allow_unsafe_apis']
  : ['--enable-unsafe-webgpu', '--use-webgpu-adapter=swiftshader', '--enable-dawn-features=allow_unsafe_apis'];
const browser = await chromium.launch({ args });
try {
  const page = await browser.newPage();
  if (process.argv.includes('--client')) {
    page.on('console', message => console.log(`${message.type()}: ${message.text()}`));
    page.on('pageerror', error => console.log(`PAGEERROR: ${error.stack}`));
    await page.goto('http://127.0.0.1:4173/VirtualLife/');
    await page.waitForTimeout(12_000);
    console.log(JSON.stringify(await page.evaluate(() => ({ status: document.querySelector('#boot-status')?.textContent, api: window.__virtualLifeBevy?.diagnostics }))));
    await browser.close();
    process.exit();
  }
  await page.goto('http://127.0.0.1:4173/VirtualLife/assets/catalog/maps.json');
  const adapter = await page.evaluate(async () => {
    const adapter = await navigator.gpu?.requestAdapter();
    return adapter ? { vendor: adapter.info.vendor, architecture: adapter.info.architecture, description: adapter.info.description, features: [...adapter.features] } : null;
  });
  console.log(JSON.stringify({ platform: process.platform, node: process.version, adapter }));
  if (!adapter) process.exitCode = 1;
} finally { await browser.close(); }
