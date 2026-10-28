const sounds = new Set<HTMLAudioElement>();
let level = 50;

/** Original 004032a0 controls one master wave-output volume, initially 50%. */
export function gameAudio(source?: string): HTMLAudioElement {
  const audio = new Audio(source); audio.volume = level / 100; sounds.add(audio); return audio;
}
export function setMasterVolume(value: number): void {
  level = Math.min(100, Math.max(0, Math.round(value)));
  for (const audio of sounds) audio.volume = level / 100;
}
export function masterVolume(): number { return level; }
