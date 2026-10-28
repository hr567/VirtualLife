import { defineConfig, type Plugin } from 'vite';
import { createReadStream } from 'node:fs';
import { cp, readFile, stat, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';

// Disc objects live outside public/ so binary provenance and LFS tracking remain clear.
function discAssets(): Plugin {
  const root = path.resolve('assets');
  let shell: string[] = [];
  return {
    name: 'disc-assets',
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const pathname = new URL(req.url ?? '/', 'http://localhost').pathname;
        const prefix = pathname.startsWith('/VirtualLife/assets/') ? '/VirtualLife/assets/' : '/assets/';
        if (!pathname.startsWith(prefix)) return next();
        let file: string;
        try { file = path.resolve(root, decodeURIComponent(pathname.slice(prefix.length))); }
        catch { res.statusCode = 400; res.end(); return; }
        if (!file.startsWith(root + path.sep)) { res.statusCode = 403; res.end(); return; }
        try {
          const info = await stat(file);
          if (!info.isFile()) return next();
          res.setHeader('Content-Length', info.size);
          res.setHeader('Content-Type', file.endsWith('.json') ? 'application/json; charset=utf-8' : 'application/octet-stream');
          createReadStream(file).pipe(res);
        } catch { next(); }
      });
    },
    generateBundle(_options, bundle) {
      shell = Object.keys(bundle);
    },
    async closeBundle() {
      await cp(root, path.resolve('dist/assets'), { recursive: true });
      const buildHash = createHash('sha256').update(shell.join('\n'));
      const catalogs = ['assets/manifest.json', 'assets/catalog/maps.json', 'assets/catalog/rules.json', 'assets/catalog/events.json', 'assets/catalog/questionnaires.json'];
      for (const file of catalogs) buildHash.update(await readFile(file));
      const buildId = buildHash.digest('hex').slice(0, 16);
      const files = [...new Set(['index.html', ...shell, ...catalogs])];
      await writeFile('dist/shell-files.json', JSON.stringify(files));
      const worker = await readFile('public/sw.js', 'utf8');
      await writeFile('dist/sw.js', worker.replace('__BUILD_ID__', buildId));
    },
  };
}

export default defineConfig({
  base: '/VirtualLife/',
  plugins: [discAssets()],
  worker: { format: 'es' },
  build: { target: 'es2022' },
});
