import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const executable = process.platform === 'win32' ? 'wasm-bindgen.exe' : 'wasm-bindgen';
const tools = resolve(root, 'research/tools');
const local = existsSync(tools) ? readdirSync(tools)
  .filter(name => name.startsWith('wasm-bindgen-0.2.129-'))
  .map(name => resolve(tools, name, executable)).find(existsSync) : undefined;
const bindgen = process.env.WASM_BINDGEN ?? local ?? executable;

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: process.env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run('cargo', ['build', '--locked', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'virtual-life-core']);
mkdirSync(resolve(root, 'src/wasm'), { recursive: true });
run(bindgen, [resolve(root, 'target/wasm32-unknown-unknown/release/virtual_life_core.wasm'),
  '--target', 'web', '--out-dir', resolve(root, 'src/wasm'), '--out-name', 'virtual_life_core']);
