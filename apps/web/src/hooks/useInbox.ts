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
  /** Needs a connection (it creates items in other modules). */
  triage: (id: string, to: TriageTarget) => Promise<Triaged | undefined>;
  /** Works offline. */
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.inbox();

export function useInbox(online: boolean): InboxState {
  const list = useCachedList<InboxItem>("inbox:last", "item", fetchAll, online);
  const { mutate, reload } = list;

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
    (id: string, to: TriageTarget) =>
      // Journal entries go to today's local date.
      mutate(() => api.triage(id, to, to === "journal" ? localDate() : undefined), (cur) => cur.filter((i) => i.id !== id)),
    [mutate],
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
