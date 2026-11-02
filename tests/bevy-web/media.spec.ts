// Browser plugin not available: use the repository's real WebGPU Playwright run.
// These oracles verify browser-decoded lossless WebM -> Bevy composition. They
// do not certify the historical IV50 decoder's color conversion or native GUI.
import { test, expect, type Page, type JSHandle, type TestInfo } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { bmp, png, blit, differingPixels, type Pixels } from './images';
import { control, diagnostic, freeze, importScenario, open, ready, scenario, saveSelected } from './helpers';

interface MediaObject {
  sha256: string; path: string; bytes: number;
  media: { width: number; height: number; frameRate: string; frames: number; losslessEncoding: boolean; decodedFramesVerified: boolean };
}
interface Manifest { resources: Record<string, { sha256: string }>; objects: MediaObject[] }
interface Caption { text: string; segment: number; durationMs: number }
interface Epilogue {
  movie: string; video: { x: number; y: number; width: number; height: number; sourceHeight: number; scanlines: boolean };
  subtitleRect: [number, number, number, number]; fontSize: number; characterSpacing: number;
  subtitles: [Caption[], Caption[]]; segments: { startMs: number; endMs: number; color: string }[];
}
interface Movie {
  phase: string; clockMs: number; displayedVideoTimeMs: number; videoTimeMs: number;
  displayedSubtitle: number | null; subtitleIndex: number; narrationStarted: boolean;
}
interface EndingPending { kind: string; state: { phase: string; victory: boolean; movie: Movie } }
interface SaveRecord { savedAt: string; payload: string }
interface SavedPayload { state: { pending: EndingPending }; presentation?: unknown }
interface GlyphStrike { height: number; offset: number; rowBytes: number; bytesPerGlyph: number }
interface GlyphCatalog {
  formatVersion: number; pixelFormat: string; resource: string; bytes: number; glyphCount: number;
  characters: string; halfWidthCharacters: string; narrowCharacters: string; strikes: GlyphStrike[];
}
interface ReferenceVideo { video: HTMLVideoElement; canvas: HTMLCanvasElement; url: string }
interface MediaAudit { type: string; element: number; source: string; readyState: number }
declare global { interface Window { readonly __endingMediaAudit?: readonly MediaAudit[] } }

const manifest = JSON.parse(readFileSync('assets/manifest.json', 'utf8')) as Manifest;
const epilogue = (JSON.parse(readFileSync('assets/catalog/rules.json', 'utf8')) as { epilogue: Epilogue }).epilogue;
const movieObject = object(epilogue.movie);
const [rateNumerator, rateDenominator] = movieObject.media.frameRate.split('/').map(Number);
const fps = rateNumerator! / rateDenominator!;
let glyphData: {
  metadata: GlyphCatalog; bits: Buffer; index: Map<string, number>; half: Set<string>; narrow: Set<string>;
} | undefined;

function object(resource: string): MediaObject {
  const hash = manifest.resources[resource]?.sha256;
  const value = manifest.objects.find(value => value.sha256 === hash);
  if (!value) throw new Error(`Missing verified media object ${resource}`);
  return value;
}
function assertPublicMovieFixture(): void {
  const list = JSON.parse(readFileSync('output/bevy-scenarios/manifest.json', 'utf8')) as { scenarios: { id: string; source: string; statePhase?: string }[] };
  const entry = list.scenarios.find(row => row.id === 'activity-ending-movie');
  expect(entry).toBeDefined(); expect(entry!.source).toMatch(/^output\/life-probes\//);
  expect(entry!.statePhase).toBe('movie');
  const envelope = JSON.parse(readFileSync(scenario('activity-ending-movie'), 'utf8')) as SaveRecord;
  const payload = JSON.parse(envelope.payload) as SavedPayload;
  expect(payload.state.pending).toMatchObject({ kind: 'ending', state: { phase: 'movie', movie: { clockMs: 0, displayedVideoTimeMs: 0 } } });
  expect(movieObject.media).toMatchObject({ width: 640, height: 180, losslessEncoding: true, decodedFramesVerified: true });
  expect(fps).toBe(24);
}
async function ending(page: Page): Promise<EndingPending> {
  const pending = (await diagnostic(page)).snapshot.pending as EndingPending;
  expect(pending).toMatchObject({ kind: 'ending', state: { phase: 'movie' } });
  return pending;
}
async function frames(page: Page, count = 2): Promise<void> {
  await page.evaluate(async count => {
    for (let i = 0; i < count; i++) await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
  }, count);
}
async function clock(page: Page, frameMs: number): Promise<void> {
  await ready(page);
  await page.evaluate(frameMs => window.__virtualLifeBevy!.input!({ frameMs }), frameMs);
  await frames(page); await ready(page);
}
async function currentSave(page: Page): Promise<SaveRecord | null> {
  return page.evaluate(() => new Promise((resolve, reject) => {
    const opening = indexedDB.open('virtual-life-bevy', 2);
    opening.onerror = () => reject(opening.error);
    opening.onsuccess = () => {
      const db = opening.result, tx = db.transaction('saves'), request = tx.objectStore('saves').get('slot-1');
      request.onsuccess = () => resolve(request.result ?? null);
      request.onerror = () => reject(request.error);
      tx.oncomplete = () => db.close();
    };
  }));
}

async function referenceVideo(page: Page, sourceObject = movieObject): Promise<JSHandle<ReferenceVideo>> {
  const url = new URL(sourceObject.path, page.url()).href;
  return page.evaluateHandle(async ({ url, hash }) => {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Reference WebM fetch ${response.status}`);
    const bytes = await response.arrayBuffer();
    const actual = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), byte => byte.toString(16).padStart(2, '0')).join('');
    if (actual !== hash) throw new Error('Reference decoder received different WebM bytes');
    const video = document.createElement('video');
    video.hidden = true; video.muted = true; video.playsInline = true; video.preload = 'auto';
    document.body.append(video);
    const blob = URL.createObjectURL(new Blob([bytes], { type: 'video/webm' }));
    await new Promise<void>((resolve, reject) => {
      video.addEventListener('loadeddata', () => resolve(), { once: true });
      video.addEventListener('error', () => reject(new Error('Independent reference video decode failed')), { once: true });
      video.src = blob; video.load();
    });
    video.pause();
    const canvas = document.createElement('canvas');
    canvas.width = video.videoWidth; canvas.height = video.videoHeight;
    return { video, canvas, url: blob };
  }, { url, hash: sourceObject.sha256 });
}
async function decode(reference: JSHandle<ReferenceVideo>, frame: number): Promise<{ pixels: Pixels; encoded: Buffer }> {
  // Seek inside the requested frame, independently of production's boundary-
  // epsilon seek. All frames in this derived object are keyframes at 24 fps.
  const encoded = await reference.evaluate(async ({ video, canvas }, { frame, fps }) => {
    const seconds = Math.min((frame + 0.5) / fps, video.duration - 0.5 / fps);
    if (Math.abs(video.currentTime - seconds) > 0.000001) await new Promise<void>((resolve, reject) => {
      video.addEventListener('seeked', () => resolve(), { once: true });
      video.addEventListener('error', () => reject(new Error('Independent reference seek failed')), { once: true });
      video.currentTime = seconds;
    });
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context) throw new Error('Reference decoder has no 2D canvas');
    context.drawImage(video, 0, 0);
    return canvas.toDataURL('image/png').split(',')[1]!;
  }, { frame, fps });
  const bytes = Buffer.from(encoded, 'base64');
  return { pixels: png(bytes), encoded: bytes };
}
async function dispose(reference: JSHandle<ReferenceVideo>): Promise<void> {
  await reference.evaluate(({ video, url }) => { video.pause(); video.removeAttribute('src'); video.load(); video.remove(); URL.revokeObjectURL(url); });
  await reference.dispose();
}
function expectedFrame(source: Pixels, pending: EndingPending): Pixels {
  const target: Pixels = { width: 640, height: 480, rgba: new Uint8Array(640 * 480 * 4) };
  for (let i = 3; i < target.rgba.length; i += 4) target.rgba[i] = 255;
  const b = epilogue.video;
  for (let y = 0; y < b.height; y++) for (let x = 0; x < b.width; x++) {
    if (b.scanlines && y % 2) continue;
    const src = (Math.floor(y * source.height / b.height) * source.width + Math.floor(x * source.width / b.width)) * 4;
    const dst = ((y + b.y) * target.width + x + b.x) * 4;
    target.rgba.set(source.rgba.subarray(src, src + 4), dst);
  }
  const displayed = pending.state.movie.displayedSubtitle;
  if (displayed !== null) {
    const caption = epilogue.subtitles[Number(pending.state.victory)]![displayed];
    if (caption?.text) subtitle(target, caption);
  }
  return target;
}
function subtitle(target: Pixels, caption: Caption): void {
  glyphData ??= (() => {
    const metadata = JSON.parse(readFileSync('assets/catalog/glyphs.json', 'utf8')) as GlyphCatalog;
    const bits = readFileSync(object(metadata.resource).path);
    const characters = Array.from(metadata.characters);
    expect(metadata).toMatchObject({ formatVersion: 2, pixelFormat: 'mono1' });
    expect(characters.length).toBe(metadata.glyphCount);
    expect(new Set(characters).size).toBe(metadata.glyphCount);
    expect(bits.length).toBe(metadata.bytes);
    for (const strike of metadata.strikes) {
      expect(strike.rowBytes).toBe(Math.ceil(strike.height / 8));
      expect(strike.bytesPerGlyph).toBe(strike.rowBytes * strike.height);
      expect(strike.offset + metadata.glyphCount * strike.bytesPerGlyph).toBeLessThanOrEqual(bits.length);
    }
    return {
      metadata, bits, index: new Map(characters.map((ch, index) => [ch, index])),
      half: new Set(Array.from(metadata.halfWidthCharacters)), narrow: new Set(Array.from(metadata.narrowCharacters)),
    };
  })();
  const { metadata, bits, index, half, narrow } = glyphData;
  const size = epilogue.fontSize;
  const strike = [...metadata.strikes].sort((a, b) => Math.abs(a.height - size) - Math.abs(b.height - size) || a.height - b.height)[0];
  expect(strike).toBeDefined();
  const [left, top, right, bottom] = epilogue.subtitleRect;
  const color = epilogue.segments[caption.segment]!.color;
  const rgb = [1, 3, 5].map(at => parseInt(color.slice(at, at + 2), 16));
  let x = left;
  for (const ch of caption.text) {
    const character = index.has(ch) ? ch : index.has('□') ? '□' : '?';
    const glyphIndex = index.get(character);
    expect(glyphIndex, `Missing bundled bitmap glyph ${ch}`).toBeDefined();
    const sourceWidth = narrow.has(character) ? Math.max(1, Math.ceil(strike!.height / 2) - 1)
      : half.has(character) ? Math.ceil(strike!.height / 2) : strike!.height;
    const width = narrow.has(character) ? Math.max(1, Math.floor(size / 2) - 1)
      : half.has(character) ? Math.floor(size / 2) : size;
    const advance = width + 1; // The original caption requests bold text.
    // Read the bundled 1bpp cells directly. This oracle neither invokes the
    // engine's font renderer nor uses an installed/browser font.
    const sample = (gx: number, gy: number): boolean => {
      if (gx < 0 || gx >= width) return false;
      const sx = Math.floor(gx * sourceWidth / width), sy = Math.floor(gy * strike!.height / size);
      const byte = strike!.offset + glyphIndex! * strike!.bytesPerGlyph + sy * strike!.rowBytes + Math.floor(sx / 8);
      return (bits[byte]! & (128 >> (sx % 8))) !== 0;
    };
    expect(x + advance).toBeLessThanOrEqual(right);
    for (let gy = 0; gy < size && top + gy < bottom; gy++) for (let gx = 0; gx < advance && x + gx < right; gx++) {
      if (!sample(gx, gy) && !(gx > 0 && sample(gx - 1, gy))) continue;
      const to = ((top + gy) * target.width + x + gx) * 4;
      for (let c = 0; c < 3; c++) target.rgba[to + c] = rgb[c]!;
    }
    x += advance + epilogue.characterSpacing;
  }
}
async function assertMoviePixels(page: Page, reference: JSHandle<ReferenceVideo>, info: TestInfo, name: string, sourceObject = movieObject): Promise<Pixels> {
  const pending = await ending(page), frame = Math.floor(pending.state.movie.displayedVideoTimeMs * fps / 1000);
  const source = await decode(reference, frame);
  expect(source.pixels).toMatchObject({ width: sourceObject.media.width, height: sourceObject.media.height });
  const expected = expectedFrame(source.pixels, pending);
  let actual!: Pixels;
  // Diagnostics publish before WebGPU finishes presenting the new texture.
  // Keep the clock frozen and require the exact frame, rather than an old one.
  await expect.poll(async () => {
    actual = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath(`${name}-bevy.png`) }));
    return differingPixels(actual, expected);
  }, { message: `Decoded frame ${frame}, nearest-neighbor video and bundled bitmap subtitle pixels` }).toBe(0);
  const b = epilogue.video;
  for (let y = 1; y < b.height; y += 2) for (let x = 0; x < b.width; x++) {
    const at = ((y + b.y) * actual.width + x + b.x) * 4;
    if (actual.rgba[at] || actual.rgba[at + 1] || actual.rgba[at + 2]) throw new Error(`Nonblack original scanline at ${x + b.x},${y + b.y}`);
  }
  await info.attach(`${name}-independent-browser-frame-${frame}`, { body: source.encoded, contentType: 'image/png' });
  await info.attach(`${name}-clock`, { body: Buffer.from(JSON.stringify({ sourceSha256: sourceObject.sha256, frame, movie: pending.state.movie, boundary: 'Browser WebM decode to Bevy composition only; historical IV50 colors are not certified.' }, null, 2)), contentType: 'application/json' });
  return actual;
}
function originalMenu(): Pixels {
  const read = (id: string) => bmp(readFileSync(object(id).path));
  const target = read('install/bmp/pm0201.bmp');
  for (let i = 0; i < 4; i++) blit(target, read(`install/bmp/pm020${i + 2}.bmp`), 237, [143, 192, 242, 290][i]!, 0, 25);
  return target;
}

test('ending WebM frames, nearest scaling, black scanlines and fixed subtitles follow the original clock and F6 save', async ({ page }, info) => {
  assertPublicMovieFixture();
  await page.setViewportSize({ width: 640, height: 480 });
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-ending-movie');
  expect((await ending(page)).state.movie).toMatchObject({ clockMs: 0, displayedVideoTimeMs: 0, displayedSubtitle: null, narrationStarted: false });
  let reference = await referenceVideo(page);
  await assertMoviePixels(page, reference, info, 'movie-first-frame');
  await clock(page, 200);
  expect((await ending(page)).state.movie).toMatchObject({ clockMs: 200, displayedVideoTimeMs: 200, narrationStarted: false });
  await assertMoviePixels(page, reference, info, 'movie-200ms');
  await clock(page, 1);
  expect((await ending(page)).state.movie).toMatchObject({ clockMs: 201, narrationStarted: true });
  await clock(page, 501);
  expect((await ending(page)).state.movie).toMatchObject({ clockMs: 702, subtitleIndex: 1, displayedSubtitle: 0 });
  await clock(page, 1);
  const before = await ending(page);
  expect(before.state.movie).toMatchObject({ clockMs: 703, displayedVideoTimeMs: 703, displayedSubtitle: 1 });
  expect(epilogue.subtitles[Number(before.state.victory)]![1]!.text.length).toBeGreaterThan(0);
  const beforePixels = await assertMoviePixels(page, reference, info, 'movie-first-caption');
  expect(Array.from(beforePixels.rgba).some((value, i) => i % 4 !== 3 && value > 0)).toBe(true);
  await frames(page, 8); expect(await ending(page)).toEqual(before);
  const previousSave = (await currentSave(page))?.savedAt;
  await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves'); await ready(page);
  await clock(page, 10000); expect(await ending(page)).toEqual(before);
  await saveSelected(page);
  await expect.poll(async () => (await currentSave(page))?.savedAt).not.toBe(previousSave);
  const saved = JSON.parse((await currentSave(page))!.payload) as SavedPayload;
  expect(saved.state.pending).toEqual(before); expect(saved.presentation).toBeDefined();
  await dispose(reference);
  await page.reload(); await ready(page); await freeze(page); await control(page, '继续游戏'); await control(page, '读取选中槽位'); await ready(page);
  expect(await ending(page)).toEqual(before);
  reference = await referenceVideo(page);
  const restoredPixels = await assertMoviePixels(page, reference, info, 'movie-restored-caption');
  expect(differingPixels(restoredPixels, beforePixels)).toBe(0);
  await dispose(reference); expect(errors).toEqual([]);
});

test('defeat selects its original 280-frame outro and finishes at its own final frame', async ({ page }, info) => {
  const errors = await open(page); await freeze(page); await importScenario(page, 'activity-ending-movie');
  await page.setViewportSize({ width: 640, height: 480 }); await ready(page);
  const loss = object('derived/proepi/aopen03.webm');
  expect(loss.media).toMatchObject({ frames: 280, frameRate: '24/1', losslessEncoding: true, decodedFramesVerified: true });
  expect((await ending(page)).state.victory).toBe(false);
  for (let guard = 0; guard < epilogue.subtitles[0].length + 2; guard++) {
    if ((await ending(page)).state.movie.phase === 'outro') break;
    await clock(page, 10_000);
  }
  expect((await ending(page)).state.movie).toMatchObject({ phase: 'outro', videoTimeMs: 0, displayedVideoTimeMs: 0, displayedSubtitle: null });
  const reference = await referenceVideo(page, loss);
  await assertMoviePixels(page, reference, info, 'defeat-outro-first', loss);
  await clock(page, 11_666);
  expect((await ending(page)).state.movie).toMatchObject({ phase: 'outro', videoTimeMs: 11_666, displayedVideoTimeMs: 11_666 });
  await assertMoviePixels(page, reference, info, 'defeat-outro-last', loss);
  await clock(page, 1);
  const pending = (await diagnostic(page)).snapshot.pending as EndingPending | null;
  expect(pending?.kind !== 'ending' || pending.state.phase !== 'movie').toBe(true);
  await dispose(reference); expect(errors).toEqual([]);
});

test.describe('controlled media network', () => {
// page.route does not intercept Service-Worker-owned requests. Isolate only
// this network-delay test; the normal media case and offline suite retain SW.
// https://playwright.dev/docs/api/class-page#page-route
test.use({ serviceWorkers: 'block' });
test('a delayed real ending decode cannot replace the menu after the actual F10 exit', async ({ page }, info) => {
  assertPublicMovieFixture();
  await page.setViewportSize({ width: 640, height: 480 });
  // Observe native media readiness only. The wrapper calls the original load
  // unchanged and never replaces a game API, message, command or domain state.
  await page.addInitScript(() => {
    const events: MediaAudit[] = [], tracked = new WeakMap<HTMLMediaElement, number>();
    let serial = 0;
    Object.defineProperty(window, '__endingMediaAudit', { get: () => Object.freeze(events.map(event => Object.freeze({ ...event }))) });
    const load = HTMLMediaElement.prototype.load;
    HTMLMediaElement.prototype.load = function () {
      if (this instanceof HTMLVideoElement && !tracked.has(this)) {
        tracked.set(this, ++serial);
        for (const type of ['loadeddata', 'emptied', 'error']) this.addEventListener(type, () => events.push({ type, element: tracked.get(this)!, source: this.currentSrc || this.getAttribute('src') || '', readyState: this.readyState }));
      }
      return load.call(this);
    };
  });
  const errors = await open(page); await freeze(page);
  let release!: () => void, requests = 0, delivered = false;
  const gate = new Promise<void>(resolve => { release = resolve; });
  const match = new RegExp(`/${movieObject.path.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?:\\?.*)?$`);
  await page.route(match, async route => {
    requests++; await gate;
    await route.fulfill({ status: 200, contentType: 'video/webm', body: readFileSync(movieObject.path) });
    delivered = true;
  });
  try {
    // importScenario's final ready() intentionally cannot finish while this
    // object's response is held, so perform the same real F6/file-picker steps.
    await page.keyboard.press('F6'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Saves');
    const picking = page.waitForEvent('filechooser'); await control(page, '导入槽位 1');
    await (await picking).setFiles(scenario('activity-ending-movie'));
    await expect.poll(async () => (await diagnostic(page)).notice).toContain('存档已导入');
    await control(page, '读取选中槽位');
    await expect.poll(async () => (await diagnostic(page)).screen).toBe('Game');
    await expect.poll(() => requests).toBeGreaterThan(0);
    expect((await diagnostic(page)).loading).toBe(true);
    const pending = await ending(page);
    await page.keyboard.press('F10'); await expect.poll(async () => (await diagnostic(page)).screen).toBe('Menu'); await ready(page);
    await page.mouse.move(5, 5); await frames(page);
    const before = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('menu-before-late-video.png') }));
    expect(differingPixels(before, originalMenu())).toBe(0);
    release(); await expect.poll(() => delivered).toBe(true);
    // Closing a pending VideoFrames loader waits for the genuine loadeddata,
    // then resets its element. Waiting for that emptied edge proves this is a
    // late decoder completion, rather than an arbitrary sleep or an early menu.
    await expect.poll(async () => page.evaluate(() => {
      const events = window.__endingMediaAudit ?? [];
      return events.some((event, index) => event.type === 'emptied' && events.slice(0, index).some(earlier => earlier.element === event.element && earlier.type === 'loadeddata'));
    })).toBe(true);
    await frames(page); await ready(page);
    expect((await diagnostic(page)).screen).toBe('Menu'); expect(await ending(page)).toEqual(pending);
    const after = png(await page.locator('#game-canvas').screenshot({ path: info.outputPath('menu-after-late-video.png') }));
    expect(differingPixels(after, before)).toBe(0);
    await info.attach('late-video-native-media-events', { body: Buffer.from(JSON.stringify(await page.evaluate(() => window.__endingMediaAudit), null, 2)), contentType: 'application/json' });
    expect(errors).toEqual([]);
  } finally { release(); await page.unroute(match); }
});
});
