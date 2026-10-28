import type { Assets } from './assets';
import type { DecodedBitmap, GameMap, MapBackground, Point } from './types';

const shader = `
struct Rect { rect: vec4f, viewport: vec4f, uv: vec4f, effect: vec4f, color: vec4f };
@group(0) @binding(0) var<uniform> bounds: Rect;
@group(0) @binding(1) var image: texture_2d<f32>;
@group(0) @binding(2) var nearestSampler: sampler;
@group(0) @binding(3) var paletteIndices: texture_2d<u32>;
@group(0) @binding(4) var paletteOverrides: texture_2d<f32>;
struct Out { @builtin(position) position: vec4f, @location(0) uv: vec2f };
@vertex fn vertexMain(@builtin(vertex_index) vertex: u32) -> Out {
  let corners = array<vec2f, 6>(vec2f(0,0), vec2f(1,0), vec2f(0,1), vec2f(0,1), vec2f(1,0), vec2f(1,1));
  let uv = corners[vertex];
  let pixel = bounds.rect.xy + uv * bounds.rect.zw;
  var output: Out;
  output.position = vec4f(pixel.x / bounds.viewport.x * 2.0 - 1.0, 1.0 - pixel.y / bounds.viewport.y * 2.0, 0, 1);
  output.uv = bounds.uv.xy + uv * bounds.uv.zw;
  return output;
}
@fragment fn fragmentMain(input: Out) -> @location(0) vec4f {
  let sampled = textureSample(image, nearestSampler, input.uv);
  let size = textureDimensions(paletteIndices);
  let index = f32(textureLoad(paletteIndices, vec2i(input.uv * vec2f(size)), 0).r);
  let replacement = textureLoad(paletteOverrides, vec2i(i32(index), 0), 0);
  let color = select(sampled.rgb, replacement.rgb, replacement.a > 0.0);
  let factor = select(vec3f(1), bounds.color.xyz, index >= bounds.effect.x && index <= bounds.effect.y);
  return vec4f(floor(color * 255.0 * factor + vec3f(0.0001)) / 255.0, sampled.a);
}`;

interface Sprite { texture: GPUTexture; indices: GPUTexture; uniform: GPUBuffer; bindGroup: GPUBindGroup; tile: MapBackground; uv: [number, number, number, number]; screenSpace: boolean; zIndex?: number; mapLayer?: 'spirit' | 'attached' | 'decoration' | 'foreground'; clip?: { x: number; y: number; width: number; height: number } }
interface Pattern { sprite: Sprite; frames: [DecodedBitmap, DecodedBitmap]; scrollMode: number; counter: number; x: number; y: number; direction: number }
export interface SceneImage { resourceId: string; x: number; y: number; width: number; height: number; frames?: number; transparentPaletteIndex?: number; source?: { x: number; y: number; width: number; height: number } }

export class GameRenderer {
  readonly width = 640;
  readonly height = 480;
  readonly camera: Point = { x: 0, y: 0 };
  map: GameMap | undefined;
  #sprites: Sprite[] = [];
  #overlays: Sprite[] = [];
  #animated = new Map<string, Sprite>();
  #pattern: Pattern | undefined;
  #patternTimer = 0;
  #actor: Sprite | undefined;
  #pendingFrame = 0;
  #loadGeneration = 0;
  #overlayGeneration = 0;
  #disposed = false;
  #frameOffset: Point = { x: 0, y: 0 };
  #paletteEffect: [number, number, number, number, number] = [0, 255, 1, 1, 1];
  #paletteOverrides: GPUTexture;

  private constructor(
    readonly canvas: HTMLCanvasElement,
    private device: GPUDevice,
    private context: GPUCanvasContext,
    private pipeline: GPURenderPipeline,
    private sampler: GPUSampler,
  ) {
    this.#paletteOverrides = device.createTexture({ size: [256, 1], format: 'rgba8unorm', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
    this.paletteColors();
  }

  static async create(canvas: HTMLCanvasElement, onLost: (message: string) => void): Promise<GameRenderer> {
    if (!window.isSecureContext) throw new Error('请通过 HTTPS 或 localhost 打开游戏。');
    if (!navigator.gpu) throw new Error('此浏览器未提供 WebGPU。请使用支持 WebGPU 的 Chrome、Edge 或 Safari。');
    const adapter = await navigator.gpu.requestAdapter({ powerPreference: 'low-power' });
    if (!adapter) throw new Error('无法取得 WebGPU 显卡设备，请检查浏览器和显卡支持。');
    const device = await adapter.requestDevice();
    const context = canvas.getContext('webgpu');
    if (!context) { device.destroy(); throw new Error('无法创建 WebGPU 画面。'); }
    canvas.width = 640;
    canvas.height = 480;
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device, format, alphaMode: 'opaque' });
    const module = device.createShaderModule({ code: shader, label: 'Original 2D sprites' });
    const pipeline = await device.createRenderPipelineAsync({
      label: 'Original 640×480 image plane', layout: 'auto',
      vertex: { module, entryPoint: 'vertexMain' },
      fragment: { module, entryPoint: 'fragmentMain', targets: [{ format, blend: { color: { srcFactor: 'src-alpha', dstFactor: 'one-minus-src-alpha' }, alpha: { srcFactor: 'one', dstFactor: 'one-minus-src-alpha' } } }] },
      primitive: { topology: 'triangle-list' },
    });
    const renderer = new GameRenderer(canvas, device, context, pipeline, device.createSampler({ magFilter: 'nearest', minFilter: 'nearest' }));
    document.addEventListener('visibilitychange', () => {
      clearTimeout(renderer.#patternTimer); renderer.#patternTimer = 0;
      if (document.visibilityState === 'visible') renderer.#schedulePattern();
    });
    const failed = (message: string): void => {
      if (renderer.#disposed) return;
      renderer.#disposed = true;
      cancelAnimationFrame(renderer.#pendingFrame);
      onLost(message);
    };
    void device.lost.then(info => failed(info.message || '显示设备暂时不可用，游戏进度已保留。'));
    device.addEventListener('uncapturederror', (event: GPUUncapturedErrorEvent) => failed(event.error.message));
    renderer.draw();
    return renderer;
  }

  #sprite(image: DecodedBitmap, tile: MapBackground, sampler = this.sampler): Sprite {
    const texture = this.device.createTexture({
      label: tile.resourceId, size: [image.width, image.height], format: 'rgba8unorm',
      usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST,
    });
    this.device.queue.writeTexture({ texture }, image.pixels, { bytesPerRow: image.width * 4 }, [image.width, image.height]);
    const indices = this.device.createTexture({ size: image.paletteIndices ? [image.width, image.height] : [1, 1], format: 'r8uint', usage: GPUTextureUsage.TEXTURE_BINDING | GPUTextureUsage.COPY_DST });
    this.device.queue.writeTexture({ texture: indices }, image.paletteIndices ?? new Uint8Array([0]), { bytesPerRow: image.paletteIndices ? image.width : 1 }, [indices.width, indices.height]);
    const uniform = this.device.createBuffer({ size: 80, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const bindGroup = this.device.createBindGroup({
      layout: this.pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: uniform } }, { binding: 1, resource: texture.createView() }, { binding: 2, resource: sampler }, { binding: 3, resource: indices.createView() }, { binding: 4, resource: this.#paletteOverrides.createView() }],
    });
    return { texture, indices, uniform, bindGroup, tile, uv: [0, 0, 1, 1], screenSpace: false };
  }

  async setScene(images: SceneImage[], assets: Assets): Promise<void> {
    const generation = ++this.#loadGeneration;
    const decoded = await Promise.all(images.map(async image => ({ image, bitmap: await assets.bitmap(image.resourceId, image.transparentPaletteIndex) })));
    if (generation !== this.#loadGeneration || this.#disposed) return;
    this.#clearSprites();
    this.map = undefined;
    this.camera.x = this.camera.y = 0;
    for (const { image, bitmap } of decoded) {
      const sprite = this.#sprite(bitmap, { ...image, row: 0, column: 0 });
      sprite.uv[3] = 1 / (image.frames ?? 1);
      if (image.source) sprite.uv = [image.source.x / bitmap.width, image.source.y / bitmap.height, image.source.width / bitmap.width, image.source.height / bitmap.height];
      this.#sprites.push(sprite);
    }
    this.draw();
  }

  async setOverlays(images: SceneImage[], assets: Assets): Promise<void> {
    const generation = ++this.#overlayGeneration;
    const decoded = await Promise.all(images.map(async image => ({ image, bitmap: await assets.bitmap(image.resourceId, image.transparentPaletteIndex) })));
    if (generation !== this.#overlayGeneration || this.#disposed) return;
    for (const sprite of this.#overlays) { sprite.texture.destroy(); sprite.indices.destroy(); sprite.uniform.destroy(); }
    this.#overlays = decoded.map(({ image, bitmap }) => {
      const sprite = this.#sprite(bitmap, { ...image, row: 0, column: 0 });
      sprite.screenSpace = true;
      sprite.uv[3] = 1 / (image.frames ?? 1);
      if (image.source) sprite.uv = [image.source.x / bitmap.width, image.source.y / bitmap.height, image.source.width / bitmap.width, image.source.height / bitmap.height];
      return sprite;
    });
    this.draw();
  }

  /** FUN_0040a950/0040a610: two 80×80 pattern frames, tiled and advanced every 250 ms. */
  async setPattern(resourceId: string, scrollMode: number, assets: Assets): Promise<void> {
    const generation = this.#loadGeneration;
    const bitmap = await assets.bitmap(resourceId);
    if (generation !== this.#loadGeneration || this.#disposed) return;
    if (bitmap.width !== 80 || bitmap.height !== 160) throw new Error('原版事件底纹应为 80 × 160。');
    this.#clearPattern();
    const frames = [0, 1].map(index => ({ width: 80, height: 80, pixels: bitmap.pixels.slice(index * 80 * 80 * 4, (index + 1) * 80 * 80 * 4) })) as [DecodedBitmap, DecodedBitmap];
    const sampler = this.device.createSampler({ magFilter: 'nearest', minFilter: 'nearest', addressModeU: 'repeat', addressModeV: 'repeat' });
    const sprite = this.#sprite(frames[0], { resourceId, x: 0, y: 0, width: 640, height: 480, row: 0, column: 0 }, sampler);
    sprite.screenSpace = true; sprite.uv = [0, 0, 8, 6];
    this.#pattern = { sprite, frames, scrollMode, counter: 0, x: 0, y: 0, direction: 1 };
    this.draw(); this.#schedulePattern();
  }

  #schedulePattern(): void {
    if (!this.#pattern || this.#patternTimer || this.#disposed || document.visibilityState === 'hidden') return;
    this.#patternTimer = window.setTimeout(() => {
      this.#patternTimer = 0;
      const pattern = this.#pattern;
      if (!pattern || this.#disposed) return;
      const previousFrame = pattern.counter > 1 ? 1 : 0;
      if (pattern.scrollMode === 3 || pattern.scrollMode === 4) pattern.x += pattern.scrollMode === 4 ? -pattern.direction : -1;
      if (pattern.scrollMode === 1 || pattern.scrollMode === 4) pattern.y++;
      else if (pattern.scrollMode === 2 || pattern.scrollMode === 3) pattern.y--;
      pattern.x %= 80; pattern.y %= 80;
      pattern.counter = (pattern.counter + 1) % 3;
      if (pattern.counter === 0 && pattern.scrollMode === 4) pattern.direction *= -1;
      const frame = pattern.counter > 1 ? 1 : 0;
      if (frame !== previousFrame) this.device.queue.writeTexture({ texture: pattern.sprite.texture }, pattern.frames[frame].pixels, { bytesPerRow: 320 }, [80, 80]);
      pattern.sprite.uv = [pattern.x / 80, pattern.y / 80, 8, 6];
      this.draw(); this.#schedulePattern();
    }, 251);
  }

  #clearPattern(): void {
    clearTimeout(this.#patternTimer); this.#patternTimer = 0;
    this.#pattern?.sprite.texture.destroy(); this.#pattern?.sprite.indices.destroy(); this.#pattern?.sprite.uniform.destroy(); this.#pattern = undefined;
  }

  setActor(image: DecodedBitmap, position: Point, resourceId: string): void {
    this.clearActor();
    this.#actor = this.#sprite(image, { resourceId, x: position.x - 100, y: position.y - 149, width: image.width, height: image.height, row: 0, column: 0 });
    this.draw();
  }
  actorFrame(image: DecodedBitmap): void {
    if (!this.#actor) return;
    this.device.queue.writeTexture({ texture: this.#actor.texture }, image.pixels, { bytesPerRow: image.width * 4 }, [image.width, image.height]);
    if (image.paletteIndices) this.device.queue.writeTexture({ texture: this.#actor.indices }, image.paletteIndices, { bytesPerRow: image.width }, [image.width, image.height]);
    this.draw();
  }
  setAnimatedSprite(id: string, image: DecodedBitmap, position: Point, zIndex = 0): void {
    this.removeAnimatedSprite(id);
    const sprite = this.#sprite(image, { resourceId: id, x: position.x, y: position.y, width: image.width, height: image.height, row: 0, column: 0 });
    sprite.screenSpace = true;
    sprite.zIndex = zIndex;
    this.#animated.set(id, sprite);
    this.draw();
  }
  setMapSprite(id: string, image: DecodedBitmap, position: Point, attached = false): void {
    this.setAnimatedSprite(id, image, position);
    const sprite = this.#animated.get(id)!;
    sprite.screenSpace = attached; sprite.mapLayer = attached ? 'attached' : 'spirit';
  }
  setMapDecoration(id: string, image: DecodedBitmap, position: Point, foreground: boolean): void {
    this.setAnimatedSprite(id, image, position);
    const sprite = this.#animated.get(id)!;
    sprite.screenSpace = false; sprite.mapLayer = foreground ? 'foreground' : 'decoration';
  }
  animatedSpriteFrame(id: string, image: DecodedBitmap): void {
    const sprite = this.#animated.get(id);
    if (!sprite) return;
    this.device.queue.writeTexture({ texture: sprite.texture }, image.pixels, { bytesPerRow: image.width * 4 }, [image.width, image.height]);
    if (image.paletteIndices) this.device.queue.writeTexture({ texture: sprite.indices }, image.paletteIndices, { bytesPerRow: image.width }, [image.width, image.height]);
    this.draw();
  }
  animatedSpritePosition(id: string, position: Point): void {
    const sprite = this.#animated.get(id);
    if (!sprite) return;
    sprite.tile.x = position.x; sprite.tile.y = position.y;
    this.draw();
  }
  animatedSpriteSource(id: string, source: { x: number; y: number; width: number; height: number }): void {
    const sprite = this.#animated.get(id);
    if (!sprite) return;
    sprite.tile.width = source.width; sprite.tile.height = source.height;
    sprite.uv = [source.x / sprite.texture.width, source.y / sprite.texture.height, source.width / sprite.texture.width, source.height / sprite.texture.height];
    this.draw();
  }
  animatedSpriteClip(id: string, clip: { x: number; y: number; width: number; height: number }): void {
    const sprite = this.#animated.get(id);
    if (sprite) { sprite.clip = clip; this.draw(); }
  }
  frameOffset(position: Point): void { this.#frameOffset = position; this.draw(); }
  paletteEffect(red = 1, green = 1, blue = 1, first = 0, last = 255): void { this.#paletteEffect = [first, last, red, green, blue]; this.draw(); }
  paletteColors(colors = new Uint8Array(256 * 4)): void {
    this.device.queue.writeTexture({ texture: this.#paletteOverrides }, colors, { bytesPerRow: 256 * 4 }, [256, 1]);
    this.draw();
  }
  removeAnimatedSprite(id: string): void {
    const sprite = this.#animated.get(id);
    sprite?.texture.destroy(); sprite?.indices.destroy(); sprite?.uniform.destroy();
    this.#animated.delete(id);
  }
  clearAnimatedSprites(): void {
    for (const id of this.#animated.keys()) this.removeAnimatedSprite(id);
    this.draw();
  }
  actorPosition(position: Point): void {
    if (!this.#actor) return;
    this.#actor.tile.x = position.x - 100;
    this.#actor.tile.y = position.y - 149;
    this.camera.x = position.x - 320;
    this.camera.y = position.y - 245;
    this.draw();
  }
  clearActor(): void {
    this.#actor?.texture.destroy(); this.#actor?.indices.destroy(); this.#actor?.uniform.destroy(); this.#actor = undefined;
  }

  setSpriteFrame(resourceId: string, frame: number, frames: number): void {
    const sprite = [...this.#sprites, ...this.#overlays].find(candidate => candidate.tile.resourceId === resourceId);
    if (!sprite) return;
    sprite.uv[1] = Math.max(0, Math.min(frames - 1, frame)) / frames;
    sprite.uv[3] = 1 / frames;
    this.draw();
  }
  sceneSpritePosition(resourceId: string, position: Point): void {
    const sprite = this.#sprites.find(candidate => candidate.tile.resourceId === resourceId);
    if (sprite) { sprite.tile.x = position.x; sprite.tile.y = position.y; this.draw(); }
  }

  async setMap(map: GameMap, assets: Assets, progress: (loaded: number, total: number) => void): Promise<void> {
    const generation = ++this.#loadGeneration;
    this.#clearSprites();
    this.map = map;
    this.center(map.dimensions.width / 2, map.dimensions.height / 2);
    let loaded = 0;
    // Bounded decoding avoids expanding the whole disc into CPU/GPU memory.
    for (let offset = 0; offset < map.backgroundTiles.length; offset += 3) {
      const tiles = map.backgroundTiles.slice(offset, offset + 3);
      const images = await Promise.all(tiles.map(async tile => ({ tile, image: await assets.bitmap(tile.resourceId) })));
      if (generation !== this.#loadGeneration || this.#disposed) return;
      for (const { tile, image } of images) this.#sprites.push(this.#sprite(image, tile));
      loaded += images.length;
      progress(loaded, map.backgroundTiles.length);
      this.draw();
    }
  }

  center(x: number, y: number): void { this.camera.x = x - this.width / 2; this.camera.y = y - this.height / 2; this.#clamp(); this.draw(); }
  pan(dx: number, dy: number): void { this.camera.x += dx; this.camera.y += dy; this.#clamp(); this.draw(); }
  #clamp(): void {
    this.camera.x = Math.max(0, Math.min(this.camera.x, (this.map?.dimensions.width ?? this.width) - this.width));
    this.camera.y = Math.max(0, Math.min(this.camera.y, (this.map?.dimensions.height ?? this.height) - this.height));
  }
  draw(): void {
    if (this.#pendingFrame || this.#disposed) return;
    this.#pendingFrame = requestAnimationFrame(() => {
      this.#pendingFrame = 0;
      if (this.#disposed) return;
      const encoder = this.device.createCommandEncoder();
      const pass = encoder.beginRenderPass({ colorAttachments: [{ view: this.context.getCurrentTexture().createView(), clearValue: { r: 0, g: 0, b: 0, a: 1 }, loadOp: 'clear', storeOp: 'store' }] });
      pass.setPipeline(this.pipeline);
      const allAnimations = [...this.#animated.values()];
      const animations = allAnimations.filter(sprite => !sprite.mapLayer).sort((left, right) => (left.zIndex ?? 0) - (right.zIndex ?? 0));
      const mapSpirits = allAnimations.filter(sprite => sprite.mapLayer === 'spirit');
      const behind = mapSpirits.filter(sprite => sprite.tile.y - this.camera.y < 190);
      const ahead = mapSpirits.filter(sprite => sprite.tile.y - this.camera.y >= 190);
      const attached = allAnimations.filter(sprite => sprite.mapLayer === 'attached');
      const decorations = allAnimations.filter(sprite => sprite.mapLayer === 'decoration');
      const foreground = allAnimations.filter(sprite => sprite.mapLayer === 'foreground');
      const sprites = [...(this.#pattern ? [this.#pattern.sprite] : []), ...this.#sprites, ...decorations, ...behind, ...(this.#actor ? [this.#actor] : []), ...attached, ...ahead, ...foreground, ...this.#overlays, ...animations];
      for (const sprite of sprites) {
        const x = sprite.tile.x - (sprite.screenSpace ? 0 : this.camera.x) + this.#frameOffset.x;
        const y = sprite.tile.y - (sprite.screenSpace ? 0 : this.camera.y) + this.#frameOffset.y;
        if (x + sprite.tile.width <= 0 || y + sprite.tile.height <= 0 || x >= this.width || y >= this.height) continue;
        const [first, last, red, green, blue] = this.#paletteEffect;
        this.device.queue.writeBuffer(sprite.uniform, 0, new Float32Array([x, y, sprite.tile.width, sprite.tile.height, this.width, this.height, 0, 0, ...sprite.uv, first, last, 0, 0, red, green, blue, 0]));
        const clip = sprite.clip ?? { x: 0, y: 0, width: this.width, height: this.height };
        pass.setScissorRect(clip.x, clip.y, clip.width, clip.height);
        pass.setBindGroup(0, sprite.bindGroup);
        pass.draw(6);
      }
      pass.end();
      this.device.queue.submit([encoder.finish()]);
    });
  }
  #clearSprites(): void { this.#overlayGeneration++; this.#clearPattern(); this.paletteColors(); for (const sprite of [...this.#sprites, ...this.#overlays]) { sprite.texture.destroy(); sprite.indices.destroy(); sprite.uniform.destroy(); } this.#sprites = []; this.#overlays = []; this.clearActor(); this.clearAnimatedSprites(); }
  dispose(): void { this.#disposed = true; this.#loadGeneration++; cancelAnimationFrame(this.#pendingFrame); this.#clearSprites(); this.#paletteOverrides.destroy(); this.device.destroy(); }
}
