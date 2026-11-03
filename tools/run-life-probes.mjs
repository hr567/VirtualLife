// Public-command routes produce reproducible new-format saves for all eight maps.
import { spawn, spawnSync } from 'node:child_process';
import { cp, mkdir, mkdtemp, realpath, rename, rm } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const build = spawnSync('cargo', ['build', '--locked', '--release', '-p', 'virtual-life-core', '--example', 'complete_life'], { stdio: 'inherit', cwd: root });
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);
const executable = resolve(root, process.env.CARGO_TARGET_DIR ?? 'target', 'release/examples', process.platform === 'win32' ? 'complete_life.exe' : 'complete_life');
async function run(args) {
  await new Promise((resolve, reject) => {
    const child = spawn(executable, args, { stdio: 'inherit', cwd: root });
    child.on('error', reject);
    child.on('exit', code => code === 0 ? resolve() : reject(new Error(`Life probe ${args.join(' ')} exited ${code}`)));
  });
}

const output = resolve(root, 'output');
await mkdir(output, { recursive: true });
const workspace = await realpath(root);
const outputPath = relative(workspace, await realpath(output));
if (outputPath.startsWith('..') || isAbsolute(outputPath)) throw new Error('Probe output must stay inside the workspace');
const staging = await mkdtemp(join(output, '.life-probes-'));
const routes = [
  { name: 'standard-1', args: ['1', '300000', '1', 'snapshots'] },
  { name: 'standard-4', args: ['1', '300000', '4', 'snapshots'] },
  { name: 'university', args: ['1', '300000', '1', 'university', 'snapshots'] },
  { name: 'lost', args: ['1', '300000', '1', 'lost', 'snapshots'] },
];
try {
  let next = 0;
  const results = await Promise.allSettled(Array.from({ length: 2 }, async () => {
    while (next < routes.length) {
      const route = routes[next++];
      await run([...route.args, '--output-dir', join(staging, route.name)]);
    }
  }));
  for (const result of results) if (result.status === 'rejected') throw result.reason;

  // Merge only fresh successful runs. Seed 4 retains the original last-write
  // precedence for shared maps, activities and summary.json.
  const merged = join(staging, 'merged');
  await cp(join(staging, 'standard-1'), merged, { recursive: true });
  await cp(join(staging, 'standard-4'), merged, { recursive: true });
  for (const route of ['university', 'lost']) {
    await cp(join(staging, route), join(merged, route), { recursive: true });
  }
  const destination = resolve(output, 'life-probes');
  if (relative(output, destination) !== 'life-probes') throw new Error('Unsafe probe destination');
  await rm(destination, { recursive: true, force: true });
  await rename(merged, destination);
} finally {
  if (dirname(staging) !== output || !basename(staging).startsWith('.life-probes-')) throw new Error('Unsafe probe staging directory');
  await rm(staging, { recursive: true, force: true });
}
