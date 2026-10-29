/** JARVIS plan types (see core `jarvis`). */
import type { EntityRef } from "./links";

export type Risk = "read" | "write" | "destructive";
export type PlanStatus = "pending" | "running" | "executed" | "failed" | "cancelled" | "expired";

export interface PlannedAction {
  command: { type: string } & Record<string, unknown>;
  summary: string;
  risk: Risk;
  /** What to know before approving (overlaps, assumptions, time not found). */
  notes?: string[];
  /** Indexes of earlier steps this one needs. */
  after?: number[];
}

/** A session or event a step made or moved (open it on the calendar). */
export interface PlanItem {
  kind: "block" | "event";
  id: string;
  date: string;
}

export interface ActionResult {
  ok: boolean;
  message: string;
  entity?: EntityRef;
  item?: PlanItem;
  /** How the step can be reversed (recorded when it ran). */
  undo?: { op: string } & Record<string, unknown>;
  /** Left out when the plan was approved. */
  skipped?: boolean;
}

export interface Plan {
  id: string;
  input: string;
  actions: PlannedAction[];
  status: PlanStatus;
  requires_approval: boolean;
  created_at: number;
  expires_at: number;
  results: ActionResult[];
  /** The plan that undid this one. */
  reversed_by?: string;
  /** The pending plan this one replaced. */
  revises?: string;
}

export const STATUS_TONE: Record<PlanStatus, string> = {
  pending: "text-accent",
  running: "text-accent",
  executed: "text-ok",
  failed: "text-bad",
  cancelled: "text-muted",
  expired: "text-muted",
};

export interface Preset {
  id: string;
  name: string;
  input: string;
  created_at: number;
}
