import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { conflictKey, type Conflict, type Op } from "../lib/sync";
import { describeError } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";
import { OUTBOX_EVENT, SYNCED_EVENT, type SyncedDetail } from "./useSync";

export type SaveStatus = "saved" | "queued" | "dirty" | "saving" | "offline" | "error";

export const SAVE_STATUS_TEXT: Record<SaveStatus, string> = {
  saved: "saved",
  queued: "saved on this device",
  dirty: "unsaved changes",
  saving: "saving…",
  offline: "offline · not saved",
  error: "save failed",
};

interface Options<D extends { updated_at: number }, F> {
  /** IndexedDB key for the offline copy; also the outbox document key (e.g. `note:<id>`). */
  cacheKey: string;
  noun: string;
  /** Fetch the document; `null` means it doesn't exist yet (e.g. a new journal day). */
  load: () => Promise<D | null>;
  /** Editable fields from a document (or defaults when it doesn't exist). */
  fieldsOf: (doc: D | null) => F;
  /** The local copy after an offline edit; keeps the server version (`updated_at`). */
  withFields: (doc: D | null, fields: F) => D;
  /** Persist fields. `expected` is the version we last saw (undefined to create or overwrite). */
  save: (fields: F, expected: number | undefined) => Promise<D>;
  /** The outbox op for a save made offline (same `expected` semantics). */
  toOp: (fields: F, expected: number | undefined) => Op;
  /** `expected` when the document doesn't exist yet (journal: 0 = create only). */
  newVersion?: number;
  /** Offline with no saved copy: start a new document instead of failing (journal days). */
  startEmptyOffline?: boolean;
  /** The server's current document from a 409 response. */
  conflictOf: (e: ApiError) => D | undefined;
  onSaved?: (doc: D) => void;
  delayMs?: number;
}

/**
 * Load a versioned document, autosave edits after a pause, and surface
 * optimistic-concurrency conflicts (DECISIONS D-014). Offline, edits are kept
 * on the device and queued as one coalesced save that syncs on reconnect;
 * a conflict found then is shown here the next time the document is open (D-018).
 */
export function useAutosaveDoc<D extends { updated_at: number }, F>(opts: Options<D, F>) {
  const { cacheKey, noun, delayMs = 800 } = opts;
  const optsRef = useRef(opts);
  optsRef.current = opts;

  const [doc, setDocState] = useState<D | null | undefined>(undefined); // undefined = loading
  const [fields, setFields] = useState<F>(() => opts.fieldsOf(null));
  const [status, setStatus] = useState<SaveStatus>("saved");
  const [conflict, setConflict] = useState<D>();
  const [loadError, setLoadError] = useState<string>();
  const [fromCache, setFromCache] = useState(false);

  const docRef = useRef<D | null | undefined>(undefined);
  const version = useRef<number | undefined>(undefined);
  const draft = useRef<F>(fields);
  const timer = useRef<number | undefined>(undefined);

  const setDoc = useCallback((d: D | null) => {
    docRef.current = d;
    setDocState(d);
  }, []);

  const cache = useCallback(
    (d: D) =>
      void localDb()
        .then((db) => db.setValue(cacheKey, d))
        .catch(() => {}),
    [cacheKey],
  );

  const showFields = useCallback((f: F) => {
    draft.current = f;
    setFields(f);
  }, []);

  const adopt = useCallback(
    (d: D) => {
      version.current = d.updated_at;
      setDoc(d);
      showFields(optsRef.current.fieldsOf(d));
      setStatus("saved");
      setConflict(undefined);
      cache(d);
    },
    [cache, setDoc, showFields],
  );

  /** Show a conflict found while syncing: my queued edit vs. the newer server copy. */
  const showConflict = useCallback(
    (c: Conflict, keepDraft: boolean) => {
      const theirs = c.theirs as unknown as D;
      if (!keepDraft) showFields(c.mine as F);
      setConflict(theirs);
      setStatus("dirty");
    },
    [showFields],
  );

  const clearConflict = useCallback(() => {
    setConflict(undefined);
    void localDb()
      .then((db) => db.deleteValue(conflictKey(cacheKey)))
      .catch(() => {});
  }, [cacheKey]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const db = await localDb().catch(() => undefined);
      const pending = db ? await db.pendingFor(cacheKey).catch(() => []) : [];
      const createdOffline = pending.some((e) => (e.op as Op).kind.endsWith(".create"));
      let d: D | null | undefined;
      let error: unknown;
      // A document created offline isn't on the server until its create replays.
      if (!createdOffline) {
        try {
          d = await optsRef.current.load();
        } catch (e) {
          reportIfUnauthorized(e);
          error = e;
        }
      }
      if (d === undefined) {
        d = db ? await db.getValue<D>(cacheKey).catch(() => undefined) : undefined;
        if (d === undefined && optsRef.current.startEmptyOffline && isNetworkError(error)) d = null;
        if (!cancelled) setFromCache(d !== undefined && !createdOffline);
      }
      if (cancelled) return;
      if (d === undefined) {
        setLoadError(describeError(error, noun));
        return;
      }
      if (d) adopt(d);
      else setDoc(null);
      if (createdOffline) version.current = undefined;

      // Edits still waiting to sync, and conflicts found while syncing.
      const queued = pending.find((e) => e.merge === cacheKey)?.op as { fields?: F } | undefined;
      if (queued?.fields) {
        showFields(queued.fields);
        setStatus("queued");
      }
      const c = db ? await db.getValue<Conflict>(conflictKey(cacheKey)).catch(() => undefined) : undefined;
      if (c && !cancelled) showConflict(c, false);
    })();
    return () => {
      cancelled = true;
    };
  }, [adopt, cacheKey, noun, setDoc, showConflict, showFields]);

  const save = useCallback(
    async (force = false) => {
      window.clearTimeout(timer.current);
      timer.current = undefined;
      const o = optsRef.current;
      const sent = draft.current;
      const expected = force ? undefined : (version.current ?? (docRef.current ? undefined : o.newVersion));
      setStatus("saving");
      const done = () => {
        // Typing may have continued while the save was in flight.
        if (draft.current !== sent) {
          setStatus("dirty");
          timer.current = window.setTimeout(() => void save(), delayMs);
          return false;
        }
        return true;
      };

      const db = await localDb().catch(() => undefined);
      const queuedBefore = db ? (await db.outbox().catch(() => [])).length > 0 : false;
      if (navigator.onLine && !queuedBefore) {
        try {
          const d = await o.save(sent, expected);
          version.current = d.updated_at;
          setDoc(d);
          if (force) clearConflict();
          else setConflict(undefined);
          o.onSaved?.(d);
          cache(d);
          if (done()) setStatus("saved");
          return;
        } catch (e) {
          reportIfUnauthorized(e);
          const current = e instanceof ApiError && e.code === "conflict" ? o.conflictOf(e) : undefined;
          if (current) {
            setConflict(current);
            setStatus("dirty");
            return;
          }
          if (!isNetworkError(e)) {
            setStatus("error");
            return;
          }
        }
      }

      // Offline (or earlier changes are still queued): keep it on the device.
      if (!db) {
        setStatus("offline");
        return;
      }
      try {
        await db.enqueueMerged(cacheKey, (queued) => {
          const op = o.toOp(sent, expected);
          // Coalesce with a save already waiting: it holds the version these edits started from.
          return queued && !force ? { ...op, base: (queued as { base?: number }).base } : op;
        });
      } catch {
        setStatus("offline");
        return;
      }
      const local = o.withFields(docRef.current ?? null, sent);
      setDoc(local);
      cache(local);
      o.onSaved?.(local);
      if (force) clearConflict();
      window.dispatchEvent(new Event(OUTBOX_EVENT));
      if (done()) setStatus("queued");
    },
    [cache, cacheKey, clearConflict, delayMs, setDoc],
  );

  // A replay saved this document (new server version) or found a conflict.
  useEffect(() => {
    const onSynced = (e: Event) => {
      const detail = (e as CustomEvent<SyncedDetail | undefined>).detail;
      const v = detail?.versions[cacheKey];
      if (v !== undefined) version.current = v;
      void (async () => {
        const db = await localDb();
        if (detail?.conflicts.includes(cacheKey)) {
          const c = await db.getValue<Conflict>(conflictKey(cacheKey));
          if (c) showConflict(c, true);
          return;
        }
        if ((await db.pendingFor(cacheKey)).length === 0) setStatus((s) => (s === "queued" ? "saved" : s));
      })().catch(() => {});
    };
    window.addEventListener(SYNCED_EVENT, onSynced);
    return () => window.removeEventListener(SYNCED_EVENT, onSynced);
  }, [cacheKey, showConflict]);

  const edit = useCallback(
    (patch: Partial<F>) => {
      draft.current = { ...draft.current, ...patch };
      setFields(draft.current);
      setStatus("dirty");
      window.clearTimeout(timer.current);
      timer.current = window.setTimeout(() => void save(), delayMs);
    },
    [delayMs, save],
  );

  // Flush a pending save when the editor unmounts (switching documents, leaving the module).
  useEffect(
    () => () => {
      if (timer.current !== undefined) void save();
    },
    [save],
  );

  return {
    /** `undefined` while loading, `null` when the document doesn't exist yet. */
    doc,
    fields,
    edit,
    status,
    conflict,
    loadError,
    /** Showing the copy saved on this device (the server was unreachable). */
    fromCache,
    /** Replace local state with a document saved elsewhere (e.g. after pinning). */
    adopt,
    useTheirs: () => {
      if (!conflict) return;
      adopt(conflict);
      clearConflict();
    },
    keepMine: () => void save(true),
    /** Drop unsaved edits (before deleting the document, so unmounting doesn't save it again). */
    discard: () => {
      window.clearTimeout(timer.current);
      timer.current = undefined;
    },
  };
}
