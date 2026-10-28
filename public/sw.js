/* A static-only service worker. Game state lives in IndexedDB, never in this cache. */
const BUILD = '__BUILD_ID__';
const SHELL = `virtual-life-shell-${BUILD}`;
const OBJECTS = 'virtual-life-disc-v1';
const base = new URL('./', self.location.href);

self.addEventListener('install', event => {
  event.waitUntil((async () => {
    const response = await fetch(new URL('shell-files.json', base), { cache: 'no-cache' });
    if (!response.ok) throw new Error('Shell manifest unavailable');
    const files = await response.json();
    const cache = await caches.open(SHELL);
    await cache.addAll(files.map(file => new URL(file, base).href));
  })());
});

self.addEventListener('activate', event => {
  // Default lifecycle waits for old clients to close; do not interrupt a running game.
  event.waitUntil((async () => {
    for (const name of await caches.keys()) {
      if (name.startsWith('virtual-life-shell-') && name !== SHELL) await caches.delete(name);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', event => {
  const url = new URL(event.request.url);
  if (event.request.method !== 'GET' || url.origin !== base.origin || !url.pathname.startsWith(base.pathname)) return;
  if (url.pathname.endsWith('/sw.js') || url.pathname.endsWith('/shell-files.json')) return;
  const isDiscObject = url.pathname.includes('/assets/game/objects/');
  event.respondWith((async () => {
    const cache = await caches.open(isDiscObject ? OBJECTS : SHELL);
    const key = event.request.mode === 'navigate' ? new URL('index.html', base).href : event.request;
    // This scope contains only same-origin static files. Vite's CORS middleware
    // adds Vary: Origin although their bytes do not vary; module requests must
    // match the installation's precache requests when the browser is offline.
    const cached = await cache.match(key, { ignoreVary: true });
    if (cached) return cached;
    const response = await fetch(event.request);
    if (response.ok && response.type === 'basic') {
      try { await cache.put(key, response.clone()); }
      catch { /* Storage pressure must not prevent online play or delete save records. */ }
    }
    return response;
  })());
});
