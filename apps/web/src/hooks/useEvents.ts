import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import type { CalendarEvent, EventPatch, NewEvent } from "../lib/calendar";
import { applyEventLocally, clientId } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface EventsState {
  events?: CalendarEvent[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (e: NewEvent) => Promise<CalendarEvent | undefined>;
  update: (id: string, patch: EventPatch) => Promise<CalendarEvent | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.events();

/** All calendar events (expanded per range on the client); editable offline (D-017). */
export function useEvents(online: boolean): EventsState {
  const list = useCachedList<CalendarEvent>("events:last", "event", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (e: NewEvent) => {
      const event = { ...e, id: clientId() };
      return runOrQueue({
        list,
        noun: "event",
        op: { kind: "event.create", event },
        server: () => api.createEvent(event),
        fromServer: (cur, created) => [...cur, created],
        applyLocal: applyEventLocally,
        local: (cur) => cur.find((x) => x.id === event.id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: EventPatch) =>
      runOrQueue({
        list,
        noun: "event",
        op: { kind: "event.update", id, patch },
        server: () => api.updateEvent(id, patch),
        fromServer: (cur, u) => cur.map((x) => (x.id === id ? u : x)),
        applyLocal: applyEventLocally,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
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

  return { events: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove };
}
