/** JARVIS plan types (see core `jarvis`). */
import type { EntityRef } from "./links";

export type Risk = "read" | "write" | "destructive";
export type PlanStatus = "pending" | "running" | "executed" | "failed" | "cancelled" | "expired";

export interface PlannedAction {
  command: { type: string } & Record<string, unknown>;
  summary: string;
  risk: Risk;
}

export interface ActionResult {
  ok: boolean;
  message: string;
  entity?: EntityRef;
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
}

export const STATUS_TONE: Record<PlanStatus, string> = {
  pending: "text-accent",
  running: "text-accent",
  executed: "text-ok",
  failed: "text-bad",
  cancelled: "text-muted",
  expired: "text-muted",
};
