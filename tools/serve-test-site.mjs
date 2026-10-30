// A dependency-free static server shared by Windows and Linux browser QA.
import { createServer } from 'node:http';
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import path from 'node:path';
const root = path.resolve(process.env.TEST_SITE_ROOT ?? 'output/test-site');
const port = Number(process.env.PORT ?? 4173);
const base = '/VirtualLife/';
const mime = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.json': 'application/json', '.wasm': 'application/wasm', '.webm': 'video/webm', '.wav': 'audio/wav' };
createServer(async (request, response) => {
  try {
    if (!['GET', 'HEAD'].includes(request.method)) { response.writeHead(405).end(); return; }
    const url = new URL(request.url, 'http://127.0.0.1');
    if (!url.pathname.startsWith(base)) { response.writeHead(404).end(); return; }
    const relative = decodeURIComponent(url.pathname.slice(base.length)) || 'index.html';
    const file = path.resolve(root, relative);
    if (!file.startsWith(root + path.sep)) { response.writeHead(403).end(); return; }
    const info = await stat(file);
    if (!info.isFile()) { response.writeHead(404).end(); return; }
    response.setHeader('Content-Type', mime[path.extname(file)] ?? 'application/octet-stream');
    response.setHeader('Cache-Control', 'no-cache');
    response.setHeader('Accept-Ranges', 'bytes');
    const range = /^bytes=(\d+)-(\d*)$/.exec(request.headers.range ?? '');
    let start = 0, end = info.size - 1;
    if (range) {
      start = Number(range[1]); end = range[2] ? Number(range[2]) : end;
      if (start > end || end >= info.size) { response.writeHead(416, { 'Content-Range': `bytes */${info.size}` }).end(); return; }
      response.statusCode = 206; response.setHeader('Content-Range', `bytes ${start}-${end}/${info.size}`);
    }
    response.setHeader('Content-Length', end - start + 1);
    if (request.method === 'HEAD') response.end();
    else createReadStream(file, { start, end }).pipe(response);
  } catch { response.writeHead(404).end(); }
}).listen(port, '127.0.0.1', () => console.log(`Static test site: http://127.0.0.1:${port}${base}`));
