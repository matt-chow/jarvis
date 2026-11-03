/** Home's at-a-glance line (D-050): what is next today, and what needs attention. */
import type { Occurrence } from "./calendar";
import { isOpen, type Task } from "./tasks";

export interface Glance {
  /** The next timed event today that has not ended. */
  next?: { title: string; start: string; now: boolean };
  /** Overdue open tasks plus overdue reminders. */
  overdue: number;
  /** Open tasks due today. */
  dueToday: number;
}

/** `occ` is today's occurrences; `now` is `HH:MM` local time. */
export function glance(occ: Occurrence[], tasks: Task[], overdueReminders: number, today: string, now: string): Glance {
  const timed = occ.filter((o) => o.start_date === today && o.event.start_time && (o.event.end_time ?? o.event.start_time) > now);
  const n = timed[0];
  const open = tasks.filter((t) => isOpen(t) && t.due_date);
  return {
    next: n && { title: n.event.title, start: n.event.start_time!, now: n.event.start_time! <= now },
    overdue: open.filter((t) => t.due_date! < today).length + overdueReminders,
    dueToday: open.filter((t) => t.due_date === today).length,
  };
}

/** Short phrases for the line under the greeting; never empty. */
export function glanceParts(g: Glance): string[] {
  const parts: string[] = [];
  if (g.next) parts.push(g.next.now ? `Now: ${g.next.title}` : `Next: ${g.next.title} at ${g.next.start}`);
  if (g.dueToday) parts.push(`${g.dueToday} due today`);
  if (g.overdue) parts.push(`${g.overdue} overdue`);
  if (parts.length === 0) parts.push("Nothing else scheduled or due today.");
  return parts;
}
