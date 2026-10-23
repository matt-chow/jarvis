import { useCallback, useEffect, useRef, useState, type DragEvent, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { useHashParam } from "../hooks/useHashParam";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { breadcrumbs, filesRoute, joinFolder, type FileEntry, type Listing, type Usage } from "../lib/files";
import { formatBytes, formatDate } from "../lib/format";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

/** The last listing of a folder seen on this device (D-032). */
interface CachedFolder {
  listing: Listing;
  usage: Usage;
}
const cacheKey = (folder: string) => `files:${folder}`;

/** Save a copy kept on this device (a downloaded book or track) under its name. */
async function downloadSaved(file: FileEntry) {
  const saved = await (await localDb()).getMedia(file.id);
  if (!saved) throw new Error(`“${file.name}” isn't saved on this device.`);
  const url = URL.createObjectURL(saved.blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = file.name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

function describe(e: unknown, name?: string): string {
  if (e instanceof ApiError) {
    if (e.code === "exists") return name ? `“${name}” already exists in this folder.` : "That name is already used in this folder.";
    if (e.code === "quota_exceeded") return "Your storage is full. Delete something to make room.";
    if (e.code === "too_large") return name ? `“${name}” is larger than the per-file limit.` : "That file is too large.";
    if (e.code === "invalid_input" && e.detail) return `Check ${e.detail.replace(":", " —")}.`;
    if (e.status === 401) return "Your session has ended. Sign in again.";
    return `Request failed (${e.code}).`;
  }
  if (isNetworkError(e)) return "Can't reach the server right now.";
  return String(e);
}

/** Private files: folders, streamed uploads/downloads, rename/move, quota. */
export function Files({ online }: ModuleProps) {
  const folder = useHashParam("folder") ?? "";
  const [listing, setListing] = useState<Listing>();
  const [usage, setUsage] = useState<Usage>();
  const [error, setError] = useState<string>();
  const [status, setStatus] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [newFolder, setNewFolder] = useState<string>();
  /** Showing the copy saved on this device. */
  const [stale, setStale] = useState(false);
  const [saved, setSaved] = useState<Map<string, number>>(new Map());
  const [loading, setLoading] = useState(true);
  const fileInput = useRef<HTMLInputElement>(null);

  const fromDevice = useCallback(async () => {
    const db = await localDb().catch(() => undefined);
    const cached = await db?.getValue<CachedFolder>(cacheKey(folder)).catch(() => undefined);
    setSaved((await db?.mediaSizes().catch(() => undefined)) ?? new Map());
    if (cached) {
      setListing(cached.listing);
      setUsage(cached.usage);
    }
    setStale(true);
    return !!cached;
  }, [folder]);

  const reload = useCallback(async () => {
    try {
      if (!online) {
        setError(undefined);
        await fromDevice();
        return;
      }
      try {
        const [l, u] = await Promise.all([api.files(folder), api.fileUsage()]);
        setListing(l);
        setUsage(u);
        setStale(false);
        setError(undefined);
        void localDb()
          .then((db) => db.setValue<CachedFolder>(cacheKey(folder), { listing: l, usage: u }))
          .catch(() => {});
      } catch (e) {
        reportIfUnauthorized(e);
        // Unreachable: the offline notice explains; other errors are shown.
        if (isNetworkError(e)) await fromDevice();
        else setError(describe(e));
      }
    } finally {
      setLoading(false);
    }
  }, [folder, online]);

  useEffect(() => {
    setListing(undefined);
    setUsage(undefined);
    setLoading(true);
    void reload();
  }, [reload]);

  const offline = !online || stale;

  const uploadAll = async (files: FileList | File[]) => {
    const list = Array.from(files);
    if (list.length === 0) return;
    setBusy(true);
    setError(undefined);
    let done = 0;
    for (const file of list) {
      setStatus(`Uploading ${file.name}${list.length > 1 ? ` (${done + 1} of ${list.length})` : ""}…`);
      try {
        await api.uploadFile(folder, file);
        done++;
      } catch (e) {
        reportIfUnauthorized(e);
        setError(describe(e, file.name));
        break;
      }
    }
    setStatus(done ? `Uploaded ${done} file${done === 1 ? "" : "s"}.` : undefined);
    setBusy(false);
    await reload();
  };

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setDragging(false);
    if (!busy && !offline) void uploadAll(e.dataTransfer.files);
  };

  const createFolder = (e: FormEvent) => {
    e.preventDefault();
    const name = newFolder?.trim();
    if (!name || name.includes("/")) return;
    setNewFolder(undefined);
    location.hash = filesRoute(joinFolder(folder, name)).slice(1);
  };

  const pct = usage && usage.quota > 0 ? Math.min(100, Math.round((usage.used / usage.quota) * 100)) : 0;
  return (
    <div onDragOver={(e) => (e.preventDefault(), setDragging(true))} onDragLeave={() => setDragging(false)} onDrop={onDrop}>
      <Panel
        title="files"
        actions={
          <span className="flex gap-1.5">
            <Button className={small} disabled={busy || offline} onClick={() => setNewFolder("")}>
              new folder
            </Button>
            <Button variant="primary" className={small} disabled={busy || offline} onClick={() => fileInput.current?.click()}>
              upload
            </Button>
          </span>
        }
      >
        <input
          ref={fileInput}
          type="file"
          multiple
          className="sr-only"
          aria-label="Upload files"
          onChange={(e) => {
            const files = e.target.files;
            if (files) void uploadAll(files);
            e.target.value = "";
          }}
        />

        <nav aria-label="Folder" className="mb-3 flex flex-wrap items-center gap-1 font-mono text-xs">
          {breadcrumbs(folder).map((c, i, all) => (
            <span key={c.path} className="flex items-center gap-1">
              {i > 0 && <span className="text-muted">/</span>}
              {i === all.length - 1 ? (
                <span className="text-ink">{c.name}</span>
              ) : (
                <a href={filesRoute(c.path)} className="text-accent hover:underline">
                  {c.name}
                </a>
              )}
            </span>
          ))}
        </nav>

        {usage && (
          <div className="mb-3">
            <div className="h-1 overflow-hidden rounded-full bg-sunken" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}>
              <div className={`h-full ${pct > 90 ? "bg-bad" : "bg-accent"}`} style={{ width: `${pct}%` }} />
            </div>
            <p className="mt-1 font-mono text-[11px] text-muted">
              {formatBytes(usage.used)} of {formatBytes(usage.quota)} used · max {formatBytes(usage.max_file)} per file
            </p>
          </div>
        )}

        {newFolder !== undefined && (
          <form onSubmit={createFolder} className="mb-3 flex gap-2">
            <input
              autoFocus
              value={newFolder}
              maxLength={200}
              onChange={(e) => setNewFolder(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setNewFolder(undefined)}
              placeholder="Folder name"
              aria-label="Folder name"
              className={`${inputClass} min-w-0 flex-1`}
            />
            <Button type="submit" className={small} disabled={!newFolder.trim() || newFolder.includes("/")}>
              create
            </Button>
          </form>
        )}

        {offline && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: {listing ? "this is the last copy of this folder seen on this device. " : ""}Files saved on this device (books and
            tracks) can be downloaded; uploads and changes need a connection.
          </p>
        )}

        {!offline && <div className={`rounded-sm border border-dashed px-3 py-2 text-center text-xs ${dragging ? "border-accent text-accent" : "border-line text-muted"}`}>
          {busy ? status : "Drop files here to upload to this folder"}
        </div>}
        {!busy && status && (
          <p role="status" className="mt-2 text-sm text-muted">
            {status}
          </p>
        )}
        {error && (
          <p role="alert" className="mt-2 text-sm text-bad">
            {error}
          </p>
        )}

        {!listing ? (
          loading ? (
            <p className="mt-4 text-sm text-muted">Loading…</p>
          ) : offline ? (
            <p className="mt-4 text-sm text-muted">This folder hasn't been opened on this device yet, so it needs a connection.</p>
          ) : (
            <Button className={`${small} mt-4`} onClick={() => void reload()}>
              retry
            </Button>
          )
        ) : listing.folders.length === 0 && listing.files.length === 0 ? (
          <p className="mt-4 text-sm text-muted">
            {folder ? "This folder is empty. Upload a file to keep it." : "No files yet. Upload something to get started."}
          </p>
        ) : (
          <ul className="mt-3 divide-y divide-line">
            {listing.folders.map((f) => (
              <li key={`d:${f}`}>
                <a href={filesRoute(joinFolder(folder, f))} className="flex items-center gap-3 py-2 text-sm hover:text-accent">
                  <span aria-hidden className="w-4 text-center font-mono text-accent">
                    ▸
                  </span>
                  <span className="truncate">{f}/</span>
                </a>
              </li>
            ))}
            {listing.files.map((f) => (
              <FileRow key={f.id} file={f} disabled={busy || offline} offline={offline} savedBytes={saved.get(f.id)} onChanged={reload} onError={setError} />
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}

function FileRow(props: {
  file: FileEntry;
  disabled: boolean;
  offline: boolean;
  /** Bytes kept on this device (a saved book or track), if any. */
  savedBytes?: number;
  onChanged: () => Promise<void>;
  onError: (m: string) => void;
}) {
  const { file, disabled, onChanged, onError } = props;
  const [mode, setMode] = useState<"view" | "rename" | "move" | "delete">("view");
  const [value, setValue] = useState("");

  const act = async (fn: () => Promise<unknown>) => {
    try {
      await fn();
      setMode("view");
      await onChanged();
    } catch (e) {
      reportIfUnauthorized(e);
      onError(describe(e, file.name));
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (mode === "rename") void act(() => api.updateFile(file.id, { name: value }));
    if (mode === "move") void act(() => api.updateFile(file.id, { folder: value }));
  };

  return (
    <li className="py-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span aria-hidden className="w-4 text-center font-mono text-muted">
          ▤
        </span>
        {!props.offline ? (
          <a href={api.fileContentUrl(file.id)} download={file.name} className="min-w-0 flex-1 truncate text-sm hover:text-accent">
            {file.name}
          </a>
        ) : props.savedBytes !== undefined ? (
          <button
            type="button"
            onClick={() => void downloadSaved(file).catch((e: Error) => onError(e.message))}
            aria-label={`Download “${file.name}” from this device`}
            className="min-w-0 flex-1 truncate text-left text-sm hover:text-accent"
          >
            {file.name}
          </button>
        ) : (
          <span className="min-w-0 flex-1 truncate text-sm text-muted">{file.name}</span>
        )}
        <span className="font-mono text-[11px] text-muted">
          {formatBytes(file.size)} · {formatDate(file.updated_at)}
          {props.offline && (props.savedBytes !== undefined ? " · on this device" : " · not on this device")}
        </span>
        {mode === "view" && (
          <span className="flex gap-1">
            <Button className={small} disabled={disabled} onClick={() => (setValue(file.name), setMode("rename"))}>
              rename
            </Button>
            <Button className={small} disabled={disabled} onClick={() => (setValue(file.folder), setMode("move"))}>
              move
            </Button>
            <Button variant="danger" className={small} disabled={disabled} onClick={() => setMode("delete")}>
              del
            </Button>
          </span>
        )}
        {mode === "delete" && (
          <span className="flex gap-1">
            <Button className={small} onClick={() => setMode("view")}>
              keep
            </Button>
            <Button variant="danger" className={small} onClick={() => void act(() => api.deleteFile(file.id))}>
              confirm delete
            </Button>
          </span>
        )}
      </div>
      {(mode === "rename" || mode === "move") && (
        <form onSubmit={submit} className="mt-2 flex gap-2 pl-7">
          <input
            autoFocus
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => e.key === "Escape" && setMode("view")}
            aria-label={mode === "rename" ? "New name" : "Move to folder"}
            placeholder={mode === "move" ? "folder/path (empty = top level)" : undefined}
            className={`${inputClass} min-w-0 flex-1`}
          />
          <Button type="submit" className={small} disabled={mode === "rename" && !value.trim()}>
            {mode}
          </Button>
          <Button className={small} onClick={() => setMode("view")}>
            cancel
          </Button>
        </form>
      )}
    </li>
  );
}
