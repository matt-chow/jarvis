/**
 * Minimal promise wrapper over IndexedDB, the on-device store for offline data.
 * Schema changes bump DB_VERSION and add a step in `upgrade`.
 */

export const DB_NAME = "directory";
export const DB_VERSION = 2;

/** A queued offline change (see lib/sync.ts); `seq` orders replay. */
export interface OutboxEntry {
  seq?: number;
  op: unknown;
  created_at: number;
}

export interface Note {
  id: string;
  body: string;
  createdAt: number;
  updatedAt: number;
}

interface Stores {
  notes: Note;
  kv: unknown;
  outbox: OutboxEntry;
}
type StoreName = keyof Stores;

function upgrade(db: IDBDatabase, oldVersion: number) {
  if (oldVersion < 1) {
    db.createObjectStore("notes", { keyPath: "id" }).createIndex("updatedAt", "updatedAt");
    db.createObjectStore("kv");
  }
  if (oldVersion < 2) {
    db.createObjectStore("outbox", { keyPath: "seq", autoIncrement: true });
  }
}

function promisify<T>(req: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export class LocalDb {
  private constructor(private readonly db: IDBDatabase) {
    // Let another tab upgrade the schema instead of blocking it.
    db.onversionchange = () => db.close();
  }

  static open(factory: IDBFactory = indexedDB, name = DB_NAME): Promise<LocalDb> {
    return new Promise((resolve, reject) => {
      const req = factory.open(name, DB_VERSION);
      req.onupgradeneeded = (e) => upgrade(req.result, e.oldVersion);
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

  /** Append to the offline outbox; returns its sequence number. */
  async enqueue(op: unknown): Promise<number> {
    const entry: OutboxEntry = { op, created_at: Date.now() };
    return (await promisify(this.store("outbox", "readwrite").add(entry))) as number;
  }

  /** Queued changes, oldest first. */
  async outbox(): Promise<OutboxEntry[]> {
    return (await promisify(this.store("outbox", "readonly").getAll())) as OutboxEntry[];
  }

  async dequeue(seq: number): Promise<void> {
    await promisify(this.store("outbox", "readwrite").delete(seq));
  }

  close() {
    this.db.close();
  }
}

let shared: Promise<LocalDb> | undefined;

/** App-wide connection, opened lazily. */
export function localDb(): Promise<LocalDb> {
  shared ??= LocalDb.open();
  return shared;
}
