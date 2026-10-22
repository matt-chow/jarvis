/** Automation types (mirrors directory-core `automations`, DECISIONS D-024). */

export type Trigger = { kind: "task_completed"; project_id?: string | null } | { kind: "habit_checked"; habit_id?: string | null };

export interface Rule {
  id: string;
  name: string;
  trigger: Trigger;
  /** JARVIS command template: may use {title}, {project}, {date}. */
  command: string;
  enabled: boolean;
  created_at: number;
  updated_at: number;
  last_run_at: number | null;
  last_ok: boolean | null;
  last_message: string | null;
}

export interface NewRule {
  name: string;
  trigger: Trigger;
  command: string;
  enabled?: boolean;
}

export type RulePatch = Partial<NewRule>;

export const TEMPLATE_HINT = "Use {title} (the task or habit), {project}, and {date}. Only add task, note, journal, and capture can run automatically.";

/** "When a task in “Garden” is completed", etc. */
export function describeTrigger(t: Trigger, names: { projects: Map<string, string>; habits: Map<string, string> }): string {
  if (t.kind === "task_completed") {
    const p = t.project_id ? names.projects.get(t.project_id) : undefined;
    return t.project_id ? `When a task in “${p ?? "a deleted project"}” is completed` : "When any task is completed";
  }
  const h = t.habit_id ? names.habits.get(t.habit_id) : undefined;
  return t.habit_id ? `When “${h ?? "a deleted habit"}” is checked in` : "When any habit is checked in";
}
