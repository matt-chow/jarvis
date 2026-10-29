import type { CalItem } from "../../lib/planner";

/** "Monday 2 November" */
export const longDay = (date: string) =>
  new Date(`${date}T12:00:00Z`).toLocaleDateString([], { weekday: "long", day: "numeric", month: "long", timeZone: "UTC" });

/** "Mon" */
export const shortDay = (date: string) => new Date(`${date}T12:00:00Z`).toLocaleDateString([], { weekday: "short", timeZone: "UTC" });

/** "2 Nov" */
export const shortDate = (date: string) => new Date(`${date}T12:00:00Z`).toLocaleDateString([], { day: "numeric", month: "short", timeZone: "UTC" });

/** "commitment", "flexible event", "work session", "fixed session", "focus block", "deadline". */
export function itemKindLabel(i: CalItem): string {
  switch (i.kind) {
    case "event":
      return `${i.fixed ? "commitment" : "flexible event"}${i.repeating ? ", repeats" : ""}`;
    case "block":
      return i.taskId ? (i.fixed ? "fixed work session" : "work session") : i.fixed ? "fixed focus block" : "focus block";
    case "deadline":
      return "task deadline";
  }
}

/**
 * Colours: commitments are solid indigo with a bar, flexible time is dashed,
 * work sessions are violet, deadlines are outlined. Tokens only (D-046).
 */
export function itemTone(i: CalItem): string {
  if (i.kind === "deadline") return `border border-line bg-panel ${i.done ? "text-muted" : "text-ink"}`;
  if (i.kind === "event") return i.fixed ? "border-l-[3px] border-accent bg-accent/15 text-ink" : "border border-dashed border-accent bg-accent/[0.07] text-ink";
  return i.fixed ? "border-l-[3px] border-violet bg-violet/20 text-ink" : "border border-dashed border-violet bg-violet/10 text-ink";
}
