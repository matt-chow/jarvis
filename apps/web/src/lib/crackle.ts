/**
 * Turntable surface noise (D-050): a synthesized crackle layered over the
 * music, generated on the device (no audio files, no network). It runs only
 * while a track is playing, never before the owner has pressed play, and is
 * separate from interface sounds. Level 0–1 maps to a quiet range on purpose:
 * at 1 it is still well under the music.
 */

let ctx: AudioContext | undefined;
let source: AudioBufferSourceNode | undefined;
let gain: GainNode | undefined;

/** The loudest the crackle gets, as linear gain. */
export const MAX_CRACKLE_GAIN = 0.12;

export const crackleGain = (level: number) => Math.min(1, Math.max(0, level)) ** 1.5 * MAX_CRACKLE_GAIN;

/** Four seconds of vinyl noise: soft hiss, sparse pops, rarer louder clicks. */
export function crackleSamples(rate: number, seconds = 4, random: () => number = Math.random): Float32Array<ArrayBuffer> {
  const out = new Float32Array(Math.floor(rate * seconds));
  let low = 0;
  for (let i = 0; i < out.length; i++) {
    // Low-passed hiss.
    low = low * 0.97 + (random() * 2 - 1) * 0.03;
    out[i] = low * 0.35;
  }
  const pops = Math.floor(seconds * 9);
  for (let p = 0; p < pops; p++) {
    const at = Math.floor(random() * (out.length - 200));
    const loud = random() < 0.12 ? 0.9 : 0.25 + random() * 0.25;
    const sign = random() < 0.5 ? -1 : 1;
    const len = 20 + Math.floor(random() * 120);
    for (let k = 0; k < len; k++) out[at + k] += sign * loud * Math.exp(-k / (len / 5)) * (k % 2 ? 0.6 : 1);
  }
  return out;
}

/** Create or wake the audio context inside a click handler (Safari needs the gesture). */
export function primeCrackle() {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
  } catch {
    /* no Web Audio */
  }
}

/** Start (or keep) the crackle at `level`; call only after a user gesture. */
export function startCrackle(level: number) {
  try {
    ctx ??= new AudioContext();
    if (ctx.state === "suspended") void ctx.resume();
    if (!gain) {
      gain = ctx.createGain();
      gain.connect(ctx.destination);
    }
    gain.gain.setTargetAtTime(crackleGain(level), ctx.currentTime, 0.08);
    if (source) return;
    const samples = crackleSamples(ctx.sampleRate);
    const buffer = ctx.createBuffer(1, samples.length, ctx.sampleRate);
    buffer.copyToChannel(samples, 0);
    source = ctx.createBufferSource();
    source.buffer = buffer;
    source.loop = true;
    source.connect(gain);
    source.start();
  } catch {
    /* no Web Audio: no crackle */
  }
}

export function stopCrackle() {
  try {
    source?.stop();
  } catch {
    /* already stopped */
  }
  source?.disconnect();
  source = undefined;
}
