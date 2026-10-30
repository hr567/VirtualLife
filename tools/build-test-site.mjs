// Test-only deterministic input/time support is built into a separate artifact.
import { spawnSync } from 'node:child_process';
import { verifyAssets } from './verify-assets.mjs';
const env = { ...process.env, VIRTUAL_LIFE_TEST_API: '1' };
for (const [file, ...args] of [
  ['tools/build-wasm.mjs'],
  ['node_modules/typescript/bin/tsc', '--noEmit'],
  ['node_modules/vite/bin/vite.js', 'build'],
]) {
  const result = spawnSync(process.execPath, [file, ...args], { stdio: 'inherit', env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
await verifyAssets('output/test-site', { published: true });
