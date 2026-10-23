import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import type { InboxItem, Triaged, TriageTarget } from "../lib/inbox";
import { applyInboxLocally, clientId } from "../lib/sync";
import { localDate } from "../lib/tasks";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface InboxState {
  items?: InboxItem[];
  loading: boolean;
  stale: boolean;
  error?: string;
  reload: () => void;
  /** Works offline: queued and synced on reconnect. */
  capture: (text: string) => Promise<InboxItem | undefined>;
  /** Works offline: `"queued"` until it syncs (D-036). */
  triage: (id: string, to: TriageTarget) => Promise<Triaged | "queued" | undefined>;
  /** Works offline. */
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.inbox();

export function useInbox(online: boolean): InboxState {
  const list = useCachedList<InboxItem>("inbox:last", "item", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const capture = useCallback(
    (text: string) => {
      const item = { id: clientId(), text };
      return runOrQueue({
        list,
        noun: "item",
        op: { kind: "inbox.capture", item },
        server: () => api.capture(text, item.id),
        fromServer: (cur, created) => [...cur, created],
        applyLocal: applyInboxLocally,
        local: (cur) => cur.find((i) => i.id === item.id)!,
      });
    },
    [list],
  );

  const triage = useCallback(
    (id: string, to: TriageTarget) => {
      // Journal entries go to today's local date (fixed now, even if it syncs tomorrow).
      const date = to === "journal" ? localDate() : undefined;
      return runOrQueue<InboxItem, Triaged | "queued">({
        list,
        noun: "item",
        op: { kind: "inbox.triage", id, to, date },
        server: () => api.triage(id, to, date),
        fromServer: (cur) => cur.filter((i) => i.id !== id),
        applyLocal: applyInboxLocally,
        local: () => "queued",
      });
    },
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "item",
        op: { kind: "inbox.delete", id },
        server: async () => {
          await api.deleteInboxItem(id);
          return true;
        },
        fromServer: (cur) => cur.filter((i) => i.id !== id),
        applyLocal: applyInboxLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  return { items: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, capture, triage, remove };
}
