import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const read = file => readFile(file, 'utf8');
const snapshot = JSON.parse(await read('docs/dependencies-snapshot.json'));
const manifest = JSON.parse(await read('package.json'));
const frozen = new Map(snapshot.results.map(result => [`${result.ecosystem}:${result.name}`, result]));
for (const [name, version] of Object.entries({ ...manifest.dependencies, ...manifest.devDependencies })) {
  assert.equal(version, frozen.get(`npm:${name}`)?.version, `${name} differs from the official stable-release snapshot`);
}
assert.equal(manifest.packageManager, `pnpm@${frozen.get('npm:pnpm')?.version}`);
const lock = await read('Cargo.lock');
const packages = lock.split('[[package]]').map(block => ({
  name: block.match(/^name = "([^"]+)"/m)?.[1],
  version: block.match(/^version = "([^"]+)"/m)?.[1],
  checksum: block.match(/^checksum = "([^"]+)"/m)?.[1],
}));
const direct = new Set();
for (const file of ['Cargo.toml', 'crates/virtual-life-core/Cargo.toml', 'crates/virtual-life-app/Cargo.toml', 'crates/virtual-life-web/Cargo.toml']) {
  let dependencySection = false;
  for (const line of (await read(file)).split(/\r?\n/)) {
    const section = line.match(/^\[(.+)\]$/);
    if (section) { dependencySection = /(?:^|\.)dependencies$/.test(section[1]); continue; }
    if (!dependencySection || /\bpath\s*=/.test(line)) continue;
    const declaration = line.match(/^([\w-]+)\s*=\s*(.+)$/);
    if (!declaration) continue;
    const [, name, value] = declaration, record = frozen.get(`cargo:${name}`);
    assert.ok(record, `${file}: ${name} is absent from the stable-release snapshot`);
    if (!/workspace\s*=\s*true/.test(value)) {
      assert.ok(value.includes(`"=${record.version}"`), `${file}: ${name} must pin exactly ${record.version}`);
    }
    const installed = packages.find(item => item.name === name && item.version === record.version);
    assert.equal(installed?.checksum, record.checksum, `${name} Cargo checksum differs from the official registry`);
    direct.add(name);
  }
}
console.log(`Verified ${direct.size} Rust dependencies, ${Object.keys(manifest.devDependencies).length} npm dependencies and pnpm against the ${snapshot.frozenOn} stable-release snapshot.`);
