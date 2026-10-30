// Query official stable releases; a registry mirror is transport fallback for npm.
import { writeFile, mkdir } from 'node:fs/promises';

async function json(url) {
  const response = await fetch(url, { headers: { 'User-Agent': 'virtual-life-dependency-audit/1.0' }, signal: AbortSignal.timeout(20_000) });
  if (!response.ok) throw new Error(`${response.status} ${url}`);
  return response.json();
}
async function npm(name) {
  const path = `${encodeURIComponent(name)}/latest`;
  let metadata;
  let registry = 'https://registry.npmjs.org/';
  try { metadata = await json(`${registry}${path}`); }
  catch { registry = 'https://registry.npmmirror.com/'; metadata = await json(`${registry}${path}`); }
  if (metadata.version.includes('-')) throw new Error(`Latest tag is a prerelease: ${name}`);
  return { ecosystem: 'npm', name, version: metadata.version, registry, engines: metadata.engines, integrity: metadata.dist?.integrity };
}
async function crate(name) {
  const compare = (left, right) => {
    const a = left.split('.').map(Number), b = right.split('.').map(Number);
    for (let i = 0; i < Math.max(a.length, b.length); i++) { const difference = (a[i] ?? 0) - (b[i] ?? 0); if (difference) return difference; }
    return 0;
  };
  try {
    const metadata = await json(`https://crates.io/api/v1/crates/${name}`);
    const latest = metadata.versions.filter(release => !release.yanked && !release.num.includes('-')).sort((a, b) => compare(a.num, b.num)).at(-1);
    if (!latest) throw new Error(`No stable version: ${name}`);
    return { ecosystem: 'cargo', name, version: latest.num, rustVersion: latest.rust_version, checksum: latest.checksum, registry: 'https://crates.io' };
  } catch { /* Sparse index is an independent official fallback. */ }
  const path = name.length === 1 ? `1/${name}` : name.length === 2 ? `2/${name}` : name.length === 3 ? `3/${name[0]}/${name}` : `${name.slice(0, 2)}/${name.slice(2, 4)}/${name}`;
  const response = await fetch(`https://index.crates.io/${path}`, { signal: AbortSignal.timeout(20_000) });
  if (!response.ok) throw new Error(`${response.status} ${name}`);
  const releases = (await response.text()).trim().split('\n').map(line => JSON.parse(line)).filter(release => !release.yanked && !release.vers.includes('-'));
  const latest = releases.sort((a, b) => compare(a.vers, b.vers)).at(-1);
  return { ecosystem: 'cargo', name, version: latest.vers, rustVersion: latest.rust_version, checksum: latest.cksum };
}
const tasks = ['@playwright/test', '@types/node', 'typescript', 'vite', 'pnpm'].map(name => () => npm(name));
tasks.push(...['bevy', 'serde', 'serde_json', 'sha2', 'encoding_rs', 'wasm-bindgen', 'wasm-bindgen-futures', 'serde-wasm-bindgen', 'js-sys', 'web-sys'].map(name => () => crate(name)));
const results = [];
let cursor = 0;
await Promise.all(Array.from({ length: 4 }, async () => {
  while (cursor < tasks.length) {
    const task = tasks[cursor++];
    try { const result = await task(); results.push(result); process.stdout.write(`${result.ecosystem} ${result.name} ${result.version}\n`); }
    catch (error) { results.push({ error: error.message }); process.stderr.write(`${error.message}\n`); }
  }
}));
await mkdir('output', { recursive: true });
await writeFile('output/dependencies-snapshot.json', JSON.stringify({ checkedAt: new Date().toISOString(), results }, null, 2) + '\n');
if (results.some(result => result.error)) process.exitCode = 1;
