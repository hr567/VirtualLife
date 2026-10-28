import { gameAudio } from './audio';
import type { Assets } from './assets';

/** Browser decoding only; the Rust clock selects every displayed movie frame. */
export class EndingMedia {
  readonly element = document.createElement('div');
  #videos = new Map<string, Promise<HTMLVideoElement>>();
  #audio = new Map<string, Promise<HTMLAudioElement>>();
  #currentVideo: HTMLVideoElement | undefined;
  #activeAudio = new Map<string, { position: number; loop: boolean }>();
  #enabled = true;
  #paused = false;
  #generation = 0;

  constructor(private assets: Assets) {
    this.element.className = 'ending-movie';
    this.element.setAttribute('aria-label', '结局影片');
  }

  async frame(resource: string, milliseconds: number, fps = 24): Promise<void> {
    const generation = this.#generation;
    let loading = this.#videos.get(resource);
    if (!loading) {
      loading = this.assets.bytes(resource).then(async bytes => {
        const video = document.createElement('video');
        video.muted = true; video.playsInline = true; video.preload = 'auto';
        video.disablePictureInPicture = true;
        const loaded = this.#event(video, 'loadeddata');
        video.src = URL.createObjectURL(new Blob([bytes], { type: 'video/webm' }));
        await loaded;
        return video;
      });
      this.#videos.set(resource, loading);
    }
    const video = await loading;
    if (generation !== this.#generation) return;
    if (video !== this.#currentVideo) {
      this.#currentVideo?.pause(); this.#currentVideo = video;
      this.element.replaceChildren(video);
    }
    const time = Math.min(Math.max(0, video.duration - 1 / fps), Math.floor(milliseconds * fps / 1000) / fps) + .000001;
    if (Math.abs(video.currentTime - time) < .00001) return;
    const sought = this.#event(video, 'seeked');
    video.currentTime = time;
    await sought;
  }

  #event(media: HTMLMediaElement, type: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const cleanup = (): void => { clearTimeout(timeout); media.removeEventListener(type, done); media.removeEventListener('error', failed); };
      const done = (): void => { cleanup(); resolve(); };
      const failed = (): void => { cleanup(); reject(new Error(`结局媒体解码失败：${media.error?.message ?? type}`)); };
      const timeout = window.setTimeout(() => { cleanup(); reject(new Error('结局媒体读取超时')); }, 30_000);
      media.addEventListener(type, done, { once: true }); media.addEventListener('error', failed, { once: true });
    });
  }

  setSound(enabled: boolean): void { this.#enabled = enabled; this.#syncSounds(); }
  pause(paused: boolean): void { this.#paused = paused; this.#syncSounds(); }
  sounds(sounds: { resource: string; position: number; loop: boolean }[]): void {
    this.#activeAudio = new Map(sounds.map(sound => [sound.resource, sound])); this.#syncSounds();
  }
  #syncSounds(): void {
    for (const [resource, loading] of this.#audio) if (!this.#enabled || this.#paused || !this.#activeAudio.has(resource)) void loading.then(audio => audio.pause());
    if (!this.#enabled || this.#paused) return;
    for (const [resource, current] of this.#activeAudio) {
      let loading = this.#audio.get(resource);
      if (!loading) {
        loading = this.assets.bytes(resource).then(bytes => {
          const audio = gameAudio(URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))); return audio;
        });
        this.#audio.set(resource, loading);
      }
      void loading.then(audio => {
        const desired = this.#activeAudio.get(resource);
        if (!desired || !this.#enabled || this.#paused) return;
        audio.loop = current.loop;
        const duration = audio.duration && Number.isFinite(audio.duration) ? audio.duration * 1000 : undefined;
        if (!desired.loop && duration !== undefined && desired.position >= duration) { audio.pause(); return; }
        const position = duration !== undefined && desired.loop ? desired.position % duration : desired.position;
        if (audio.paused || Math.abs(audio.currentTime * 1000 - position) > 250) audio.currentTime = position / 1000;
        if (audio.paused) void audio.play().catch(() => {});
      }).catch(() => {});
    }
  }
  hide(): void {
    this.#generation++; this.element.remove(); this.#currentVideo?.pause(); this.#currentVideo = undefined;
    this.#activeAudio.clear(); this.#syncSounds();
  }
}
