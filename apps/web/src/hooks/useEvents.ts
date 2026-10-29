import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import type { CalendarEvent, EventPatch, NewEvent, SplitRequest, SplitResult } from "../lib/calendar";
import { applyEventLocally, clientId } from "../lib/sync";
import { runOptimistic, upsert } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface EventsState {
  events?: CalendarEvent[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  /** `id` re-creates a deleted event (undo). */
  create: (e: NewEvent, id?: string) => Promise<CalendarEvent | undefined>;
  update: (id: string, patch: EventPatch) => Promise<CalendarEvent | undefined>;
  remove: (id: string) => Promise<boolean>;
  /** Cancel, move, or end part of a repeating series (D-047). */
  split: (id: string, split: SplitRequest) => Promise<SplitResult | undefined>;
}

const fetchAll = () => api.events();

/**
 * All calendar events (expanded per range on the client); editable offline
 * (D-017). Changes show at once and roll back if the server refuses (D-047).
 */
export function useEvents(online: boolean): EventsState {
  const list = useCachedList<CalendarEvent>("events:last", "event", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (e: NewEvent, id = clientId()) => {
      const event = { ...e, id };
      return runOptimistic({
        list,
        noun: "event",
        op: { kind: "event.create", event },
        server: () => api.createEvent(event),
        fromServer: upsert,
        applyLocal: applyEventLocally,
        local: (cur) => cur.find((x) => x.id === id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: EventPatch) =>
      runOptimistic({
        list,
        noun: "event",
        op: { kind: "event.update", id, patch },
        server: () => api.updateEvent(id, patch),
        fromServer: upsert,
        applyLocal: applyEventLocally,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOptimistic({
        list,
        noun: "event",
        op: { kind: "event.delete", id },
        server: async () => {
          await api.deleteEvent(id);
          return true;
        },
        fromServer: (cur) => cur.filter((x) => x.id !== id),
        applyLocal: applyEventLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  const split = useCallback(
    (id: string, s: SplitRequest) =>
      runOptimistic<CalendarEvent, SplitResult>({
        list,
        noun: "event",
        op: { kind: "event.split", id, split: s },
        server: () => api.splitEvent(id, s),
        fromServer: (cur, r) => (r.replacement ? upsert(upsert(cur, r.series), r.replacement) : upsert(cur, r.series)),
        applyLocal: applyEventLocally,
        local: (cur) => ({
          series: cur.find((x) => x.id === id)!,
          replacement: s.replacement ? (cur.find((x) => x.id === s.replacement!.id) ?? null) : null,
        }),
      }),
    [list],
  );

  return { events: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove, split };
}
