import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const executable = process.platform === 'win32' ? 'wasm-bindgen.exe' : 'wasm-bindgen';
const lock = readFileSync(resolve(root, 'Cargo.lock'), 'utf8').replaceAll('\r\n', '\n');
const version = lock.match(/\[\[package\]\]\nname = "wasm-bindgen"\nversion = "([^"]+)"/)?.[1];
if (!version) throw new Error('Cargo.lock does not pin wasm-bindgen');
const tools = resolve(root, 'research/tools');
const local = existsSync(tools) ? readdirSync(tools)
  .filter(name => name.startsWith(`wasm-bindgen-${version}-`))
  .map(name => resolve(tools, name, executable)).find(existsSync) : undefined;
const bindgen = process.env.WASM_BINDGEN ?? local ?? executable;

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit', env: process.env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

const installed = spawnSync(bindgen, ['--version'], { encoding: 'utf8' });
if (installed.status !== 0 || installed.stdout.trim() !== `wasm-bindgen ${version}`) {
  throw new Error(`Install wasm-bindgen-cli ${version} to match Cargo.lock`);
}
const features = process.env.VIRTUAL_LIFE_TEST_API === '1' ? ['--features', 'test-api'] : [];
run('cargo', ['build', '--locked', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'virtual-life-web', ...features]);
mkdirSync(resolve(root, 'web/wasm'), { recursive: true });
run(bindgen, [resolve(root, 'target/wasm32-unknown-unknown/release/virtual_life_web.wasm'),
  '--target', 'web', '--out-dir', resolve(root, 'web/wasm'), '--out-name', 'virtual_life_web']);
