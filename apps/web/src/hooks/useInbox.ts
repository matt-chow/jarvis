import { useCallback } from "react";
import { api } from "../lib/api";
import type { InboxItem, Triaged, TriageTarget } from "../lib/inbox";
import { localDate } from "../lib/tasks";
import { useCachedList } from "./useCachedList";

export interface InboxState {
  items?: InboxItem[];
  loading: boolean;
  stale: boolean;
  error?: string;
  reload: () => void;
  capture: (text: string) => Promise<InboxItem | undefined>;
  triage: (id: string, to: TriageTarget) => Promise<Triaged | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.inbox();

export function useInbox(online: boolean): InboxState {
  const list = useCachedList<InboxItem>("inbox:last", "item", fetchAll, online);
  const { mutate } = list;

  const capture = useCallback((text: string) => mutate(() => api.capture(text), (cur, item) => [...cur, item]), [mutate]);

  const triage = useCallback(
    (id: string, to: TriageTarget) =>
      // Journal entries go to today's local date.
      mutate(() => api.triage(id, to, to === "journal" ? localDate() : undefined), (cur) => cur.filter((i) => i.id !== id)),
    [mutate],
  );

  const remove = useCallback(
    async (id: string) =>
      (await mutate(
        async () => {
          await api.deleteInboxItem(id);
          return true;
        },
        (cur) => cur.filter((i) => i.id !== id),
      )) ?? false,
    [mutate],
  );

  return { items: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, capture, triage, remove };
}
