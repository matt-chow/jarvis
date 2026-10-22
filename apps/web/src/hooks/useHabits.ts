import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import { withCheck, type HabitPatch, type HabitView, type NewHabit } from "../lib/habits";
import { applyHabitLocally, clientId, type Op } from "../lib/sync";
import { localDate } from "../lib/tasks";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface HabitsState {
  habits?: HabitView[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (h: NewHabit) => Promise<HabitView | undefined>;
  update: (id: string, patch: HabitPatch) => Promise<HabitView | undefined>;
  remove: (id: string) => Promise<boolean>;
  /** Mark a day done or not (idempotent). */
  check: (id: string, date: string, done: boolean) => Promise<boolean>;
}

const fetchAll = () => api.habits(localDate());
const applyLocal = (cur: HabitView[], op: Op) => applyHabitLocally(cur, op, localDate());

/** Habits with a year of check-ins; everything works offline (DECISIONS D-017). */
export function useHabits(online: boolean): HabitsState {
  const list = useCachedList<HabitView>("habits:last", "habit", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (h: NewHabit) => {
      const habit = { ...h, id: clientId() };
      return runOrQueue({
        list,
        noun: "habit",
        op: { kind: "habit.create", habit },
        server: async (): Promise<HabitView> => ({ ...(await api.createHabit(habit)), checkins: [], streak: 0 }),
        fromServer: (cur, created) => [...cur, created],
        applyLocal,
        local: (cur) => cur.find((x) => x.id === habit.id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: HabitPatch) =>
      runOrQueue({
        list,
        noun: "habit",
        op: { kind: "habit.update", id, patch },
        server: async () => {
          const h = await api.updateHabit(id, patch);
          const cur = list.latest().find((x) => x.id === id);
          const today = localDate();
          // New schedule: recompute the streak from the check-ins we have.
          return withCheck({ ...h, checkins: cur?.checkins ?? [], streak: 0 }, today, cur?.checkins.includes(today) ?? false, today);
        },
        fromServer: (cur, u) => cur.map((x) => (x.id === id ? u : x)),
        applyLocal,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "habit",
        op: { kind: "habit.delete", id },
        server: async () => {
          await api.deleteHabit(id);
          return true;
        },
        fromServer: (cur) => cur.filter((x) => x.id !== id),
        applyLocal,
        local: () => true,
      })) ?? false,
    [list],
  );

  const check = useCallback(
    async (id: string, date: string, done: boolean) =>
      (await runOrQueue({
        list,
        noun: "check-in",
        op: { kind: "habit.check", id, date, done },
        server: async () => {
          await api.checkHabit(id, date, done);
          return true;
        },
        fromServer: (cur) => cur.map((x) => (x.id === id ? withCheck(x, date, done, localDate()) : x)),
        applyLocal,
        local: () => true,
      })) ?? false,
    [list],
  );

  return { habits: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove, check };
}
