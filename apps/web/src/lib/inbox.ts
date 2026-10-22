/** Inbox types. */
import type { JournalEntry } from "./journal";
import type { Note } from "./notes";
import type { Task } from "./tasks";

export interface InboxItem {
  id: string;
  text: string;
  created_at: number;
}

export type TriageTarget = "task" | "note" | "journal";

export type Triaged =
  | { kind: "task"; task: Task }
  | { kind: "note"; note: Note }
  | { kind: "journal"; entry: JournalEntry };
