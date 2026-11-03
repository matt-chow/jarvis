/**
 * Per-device display and sound settings (D-050): information density, the
 * interface-sounds switch, and the turntable's preferences. Stored in
 * localStorage (nothing secret, nothing to sync: a phone and a desk screen
 * want different density). Every read falls back to the default when storage
 * is unavailable.
 */

import { useEffect, useState } from "react";

export type Density = "comfortable" | "compact";

export interface DeviceSettings {
  density: Density;
  /** Short interface sounds (complete, check in). Off unless switched on. */
  uiSounds: boolean;
  /** Turntable: master volume 0–1. */
  volume: number;
  /** Turntable: surface noise on/off, and its level 0–1. */
  crackle: boolean;
  crackleLevel: number;
  /** Desktop: the JARVIS side panel stays open across screens. */
  jarvisPanel: boolean;
  /** Tasks: the calendar shown beside the list on wide screens. */
  splitCalendar: boolean;
}

export const DEVICE_DEFAULTS: DeviceSettings = {
  density: "comfortable",
  uiSounds: false,
  volume: 0.8,
  crackle: false,
  crackleLevel: 0.25,
  jarvisPanel: false,
  splitCalendar: false,
};

const KEY = "device:settings";
const EVENT = "directory:device-settings";

const unit = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : d);
const bool = (v: unknown, d: boolean) => (typeof v === "boolean" ? v : d);

/** Validate a stored value field by field; unknown or broken fields take the default. */
export function parseDeviceSettings(raw: string | null): DeviceSettings {
  let v: Record<string, unknown> = {};
  try {
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) v = parsed as Record<string, unknown>;
  } catch {
    /* corrupt: defaults */
  }
  const d = DEVICE_DEFAULTS;
  return {
    density: v.density === "compact" ? "compact" : "comfortable",
    uiSounds: bool(v.uiSounds, d.uiSounds),
    volume: unit(v.volume, d.volume),
    crackle: bool(v.crackle, d.crackle),
    crackleLevel: unit(v.crackleLevel, d.crackleLevel),
    jarvisPanel: bool(v.jarvisPanel, d.jarvisPanel),
    splitCalendar: bool(v.splitCalendar, d.splitCalendar),
  };
}

export function readDevice(): DeviceSettings {
  try {
    return parseDeviceSettings(localStorage.getItem(KEY));
  } catch {
    return { ...DEVICE_DEFAULTS };
  }
}

export function writeDevice(patch: Partial<DeviceSettings>): DeviceSettings {
  const next = { ...readDevice(), ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* private mode: this session only */
  }
  applyDensity(next.density);
  window.dispatchEvent(new CustomEvent(EVENT, { detail: next }));
  return next;
}

export function applyDensity(density: Density = readDevice().density) {
  document.documentElement.dataset.density = density;
}

/** The current device settings, updated when any screen changes them. */
export function useDevice(): [DeviceSettings, (patch: Partial<DeviceSettings>) => void] {
  const [value, setValue] = useState(readDevice);
  useEffect(() => {
    const on = (e: Event) => setValue((e as CustomEvent<DeviceSettings>).detail);
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  return [value, (patch) => setValue(writeDevice(patch))];
}
