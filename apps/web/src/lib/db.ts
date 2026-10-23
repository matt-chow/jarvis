/**
 * Minimal promise wrapper over IndexedDB, the on-device store for offline data.
 * Schema changes bump DB_VERSION and add a step in `upgrade`.
 */

export const DB_NAME = "directory";
export const DB_VERSION = 3;

/** A queued offline change (see lib/sync.ts); `seq` orders replay. */
export interface OutboxEntry {
  seq?: number;
  op: unknown;
  created_at: number;
  /** The document this change touches (e.g. `note:<id>`), for editors to find pending edits. */
  doc?: string;
  /** Set on coalesced saves: later edits to the same document update this entry in place. */
  merge?: string;
  /** Bumped on each in-place update, so a replay can tell it sent a stale copy. */
  rev?: number;
}

export interface Note {
  id: string;
  body: string;
  createdAt: number;
  updatedAt: number;
}

/** A track saved for offline playback (an explicit download, not a cache). */
export interface SavedMedia {
  id: string;
  blob: Blob;
  size: number;
  saved_at: number;
}

interface Stores {
  notes: Note;
  kv: unknown;
  outbox: OutboxEntry;
  media: SavedMedia;
}
type StoreName = keyof Stores;

/**
 * Create whatever stores are missing. Keyed on what exists rather than on
 * `oldVersion`, so a database created empty by something else (an extension,
 * devtools, a half-finished delete) still ends up complete.
 */
function upgrade(db: IDBDatabase) {
  const missing = (name: StoreName) => !db.objectStoreNames.contains(name);
  if (missing("notes")) db.createObjectStore("notes", { keyPath: "id" }).createIndex("updatedAt", "updatedAt");
  if (missing("kv")) db.createObjectStore("kv");
  if (missing("outbox")) db.createObjectStore("outbox", { keyPath: "seq", autoIncrement: true });
  if (missing("media")) db.createObjectStore("media", { keyPath: "id" });
}

function promisify<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export class LocalDb {
  /** Called once if the connection is closed for us (upgrade or delete elsewhere). */
  onclose?: () => void;

  private constructor(private readonly db: IDBDatabase) {
    // Let another tab upgrade (or a "forget" delete) proceed instead of blocking it.
    db.onversionchange = () => {
      db.close();
      this.onclose?.();
    };
    db.onclose = () => this.onclose?.();
  }

  static open(factory: IDBFactory = indexedDB, name = DB_NAME): Promise<LocalDb> {
    return new Promise((resolve, reject) => {
      const req = factory.open(name, DB_VERSION);
      req.onupgradeneeded = () => upgrade(req.result);
      req.onsuccess = () => resolve(new LocalDb(req.result));
      req.onerror = () => reject(req.error);
      req.onblocked = () => reject(new Error("IndexedDB upgrade blocked by another tab"));
    });
  }

  private store(name: StoreName, mode: IDBTransactionMode) {
    return this.db.transaction(name, mode).objectStore(name);
  }

  async putNote(note: Note): Promise<void> {
    await promisify(this.store("notes", "readwrite").put(note));
  }

  async deleteNote(id: string): Promise<void> {
    await promisify(this.store("notes", "readwrite").delete(id));
  }

  /** Most recently updated first. */
  async listNotes(): Promise<Note[]> {
    const notes = await promisify(this.store("notes", "readonly").index("updatedAt").getAll());
    return (notes as Note[]).reverse();
  }

  async getValue<T>(key: string): Promise<T | undefined> {
    return (await promisify(this.store("kv", "readonly").get(key))) as T | undefined;
  }

  async setValue<T>(key: string, value: T): Promise<void> {
    await promisify(this.store("kv", "readwrite").put(value, key));
  }

  async deleteValue(key: string): Promise<void> {
    await promisify(this.store("kv", "readwrite").delete(key));
  }

  /** Append to the offline outbox; returns its sequence number. */
  async enqueue(op: unknown, doc?: string): Promise<number> {
    const entry: OutboxEntry = { op, created_at: Date.now(), doc };
    return (await promisify(this.store("outbox", "readwrite").add(entry))) as number;
  }

  /**
   * Queue a save that coalesces with an earlier queued save of the same
   * document (`key`): `update` gets the queued op (if any) and returns the new one.
   */
  async enqueueMerged(key: string, update: (queued: unknown) => unknown): Promise<void> {
    const store = this.store("outbox", "readwrite");
    const all = (await promisify(store.getAll())) as OutboxEntry[];
    const queued = all.find((e) => e.merge === key);
    if (queued) await promisify(store.put({ ...queued, op: update(queued.op), rev: (queued.rev ?? 0) + 1 }));
    else await promisify(store.add({ op: update(undefined), created_at: Date.now(), doc: key, merge: key } satisfies OutboxEntry));
  }

  /** Queued changes for one document, oldest first. */
  async pendingFor(doc: string): Promise<OutboxEntry[]> {
    return (await this.outbox()).filter((e) => e.doc === doc);
  }

  /** Queued changes, oldest first. */
  async outbox(): Promise<OutboxEntry[]> {
    return (await promisify(this.store("outbox", "readonly").getAll())) as OutboxEntry[];
  }

  async dequeue(seq: number): Promise<void> {
    await promisify(this.store("outbox", "readwrite").delete(seq));
  }

  /**
   * Finish a replayed entry. If it was updated in place while the replay was
   * in flight, keep it (rebased with `rebase`) so the newer edit still syncs.
   */
  async settle(sent: OutboxEntry, rebase?: (op: unknown) => unknown): Promise<void> {
    if (sent.seq === undefined) return;
    const store = this.store("outbox", "readwrite");
    const now = (await promisify(store.get(sent.seq))) as OutboxEntry | undefined;
    if (!now) return;
    if ((now.rev ?? 0) === (sent.rev ?? 0) || !rebase) await promisify(store.delete(sent.seq));
    else await promisify(store.put({ ...now, op: rebase(now.op) }));
  }

  async putMedia(m: SavedMedia): Promise<void> {
    await promisify(this.store("media", "readwrite").put(m));
  }

  async getMedia(id: string): Promise<SavedMedia | undefined> {
    return (await promisify(this.store("media", "readonly").get(id))) as SavedMedia | undefined;
  }

  async deleteMedia(id: string): Promise<void> {
    await promisify(this.store("media", "readwrite").delete(id));
  }

  /** Saved tracks without their blobs: id → size. */
  async mediaSizes(): Promise<Map<string, number>> {
    const all = (await promisify(this.store("media", "readonly").getAll())) as SavedMedia[];
    return new Map(all.map((m) => [m.id, m.size]));
  }

  close() {
    this.db.close();
  }
}

let shared: Promise<LocalDb> | undefined;

/** App-wide connection, opened lazily. */
export function localDb(): Promise<LocalDb> {
  if (!shared) {
    const p = LocalDb.open().then((db) => {
      // A closed connection is useless: the next caller opens a fresh one.
      db.onclose = () => {
        if (shared === p) shared = undefined;
      };
      return db;
    });
    shared = p;
  }
  return shared;
}

/**
 * Delete everything Directory stored in this browser ("forget this device").
 * Unsynced outbox changes are lost: callers check `outbox()` first.
 */
export async function deleteLocalData(factory: IDBFactory = indexedDB, name = DB_NAME): Promise<void> {
  if (name === DB_NAME && shared) {
    const db = await shared.catch(() => undefined);
    db?.close();
    shared = undefined;
  }
  await new Promise<void>((resolve, reject) => {
    const req = factory.deleteDatabase(name);
    req.onsuccess = () => resolve();
    req.onerror = () => reject(req.error);
    // Another tab still has it open; it closes on `versionchange`, then this succeeds.
    req.onblocked = () => {};
  });
}
