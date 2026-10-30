// Public-command routes produce reproducible new-format saves for all eight maps.
import { spawn, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
const build = spawnSync('cargo', ['build', '--locked', '--release', '-p', 'virtual-life-core', '--example', 'complete_life'], { stdio: 'inherit' });
if (build.error) throw build.error;
if (build.status !== 0) process.exit(build.status ?? 1);
const executable = resolve(process.env.CARGO_TARGET_DIR ?? 'target', 'release/examples', process.platform === 'win32' ? 'complete_life.exe' : 'complete_life');
async function run(args) {
    await new Promise((resolve, reject) => {
      const child = spawn(executable, args, { stdio: 'inherit' });
      child.on('error', reject);
      child.on('exit', code => code === 0 ? resolve() : reject(new Error(`Life probe ${args.join(' ')} exited ${code}`)));
    });
}
// Standard routes share their snapshot directory, so write them in a fixed order.
await run(['1', '300000', '1', 'snapshots']);
await run(['1', '300000', '4', 'snapshots']);
await Promise.all(['university', 'lost'].map(route => run(['1', '300000', '1', route, 'snapshots'])));
