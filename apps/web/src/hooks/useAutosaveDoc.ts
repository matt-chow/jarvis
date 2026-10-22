import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { describeError } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";

export type SaveStatus = "saved" | "dirty" | "saving" | "offline" | "error";

export const SAVE_STATUS_TEXT: Record<SaveStatus, string> = {
  saved: "saved",
  dirty: "unsaved changes",
  saving: "saving…",
  offline: "offline · not saved",
  error: "save failed",
};

interface Options<D extends { updated_at: number }, F> {
  /** IndexedDB key for the offline copy of this document. */
  cacheKey: string;
  noun: string;
  /** Fetch the document; `null` means it doesn't exist yet (e.g. a new journal day). */
  load: () => Promise<D | null>;
  /** Editable fields from a document (or defaults when it doesn't exist). */
  fieldsOf: (doc: D | null) => F;
  /** Persist fields. `expected` is the version we last saw (undefined to create or overwrite). */
  save: (fields: F, expected: number | undefined) => Promise<D>;
  /** The server's current document from a 409 response. */
  conflictOf: (e: ApiError) => D | undefined;
  onSaved?: (doc: D) => void;
  delayMs?: number;
}

/**
 * Load a versioned document, autosave edits after a pause, and surface
 * optimistic-concurrency conflicts (DECISIONS D-014). Falls back to the
 * cached copy (read-only) when the server is unreachable.
 */
export function useAutosaveDoc<D extends { updated_at: number }, F>(opts: Options<D, F>) {
  const { cacheKey, noun, delayMs = 800 } = opts;
  const optsRef = useRef(opts);
  optsRef.current = opts;

  const [doc, setDoc] = useState<D | null | undefined>(undefined); // undefined = loading
  const [fields, setFields] = useState<F>(() => opts.fieldsOf(null));
  const [status, setStatus] = useState<SaveStatus>("saved");
  const [conflict, setConflict] = useState<D>();
  const [loadError, setLoadError] = useState<string>();
  const [fromCache, setFromCache] = useState(false);

  const version = useRef<number | undefined>(undefined);
  const draft = useRef<F>(fields);
  const timer = useRef<number | undefined>(undefined);

  const adopt = useCallback(
    (d: D) => {
      version.current = d.updated_at;
      const f = optsRef.current.fieldsOf(d);
      draft.current = f;
      setDoc(d);
      setFields(f);
      setStatus("saved");
      setConflict(undefined);
      void localDb()
        .then((db) => db.setValue(cacheKey, d))
        .catch(() => {});
    },
    [cacheKey],
  );

  useEffect(() => {
    let cancelled = false;
    optsRef.current
      .load()
      .then((d) => {
        if (cancelled) return;
        if (d) adopt(d);
        else setDoc(null);
      })
      .catch(async (e: unknown) => {
        reportIfUnauthorized(e);
        const cached = await localDb()
          .then((db) => db.getValue<D>(cacheKey))
          .catch(() => undefined);
        if (cancelled) return;
        if (cached) {
          adopt(cached);
          setFromCache(true);
        } else setLoadError(describeError(e, noun));
      });
    return () => {
      cancelled = true;
    };
  }, [adopt, cacheKey, noun]);

  const save = useCallback(
    async (force = false) => {
      window.clearTimeout(timer.current);
      timer.current = undefined;
      const sent = draft.current;
      setStatus("saving");
      try {
        const d = await optsRef.current.save(sent, force ? undefined : version.current);
        version.current = d.updated_at;
        setDoc(d);
        setConflict(undefined);
        optsRef.current.onSaved?.(d);
        void localDb()
          .then((db) => db.setValue(cacheKey, d))
          .catch(() => {});
        // Typing may have continued while the request was in flight.
        setStatus(draft.current === sent ? "saved" : "dirty");
        if (draft.current !== sent) timer.current = window.setTimeout(() => void save(), delayMs);
      } catch (e) {
        reportIfUnauthorized(e);
        const current = e instanceof ApiError && e.code === "conflict" ? optsRef.current.conflictOf(e) : undefined;
        if (current) {
          setConflict(current);
          setStatus("dirty");
        } else setStatus(isNetworkError(e) ? "offline" : "error");
      }
    },
    [cacheKey, delayMs],
  );

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
    fromCache,
    /** Replace local state with a document saved elsewhere (e.g. after pinning). */
    adopt,
    useTheirs: () => conflict && adopt(conflict),
    keepMine: () => void save(true),
  };
}
