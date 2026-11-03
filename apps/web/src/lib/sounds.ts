/**
 * Interface sounds (D-050): two short synthesized tones, played only when the
 * owner has switched them on (Settings › Sounds) and only in answer to their
 * own action. Separate from music and the turntable's surface noise. Every
 * sound accompanies a visible change, so nothing is conveyed by sound alone.
 */

import { readDevice } from "./device";

let ctx: AudioContext | undefined;

export type UiSound = "done" | "tap";

const TONES: Record<UiSound, { freq: number[]; ms: number; gain: number }> = {
  // A soft rising third: completing something.
  done: { freq: [660, 830], ms: 140, gain: 0.05 },
  tap: { freq: [520], ms: 60, gain: 0.03 },
};

export function playUiSound(kind: UiSound) {
  if (!readDevice().uiSounds) return;
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
    const ac = ctx;
    const t0 = ac.currentTime;
    const { freq, ms, gain } = TONES[kind];
    freq.forEach((f, i) => {
      const osc = ac.createOscillator();
      const amp = ac.createGain();
      const start = t0 + (i * ms) / 2000;
      osc.type = "sine";
      osc.frequency.value = f;
      amp.gain.setValueAtTime(0, start);
      amp.gain.linearRampToValueAtTime(gain, start + 0.01);
      amp.gain.exponentialRampToValueAtTime(0.0001, start + ms / 1000);
      osc.connect(amp).connect(ac.destination);
      osc.start(start);
      osc.stop(start + ms / 1000 + 0.02);
    });
  } catch {
    /* no Web Audio: silent */
  }
}
