import { useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { documentBytes, useLibrary, type LibraryState } from "../hooks/useLibrary";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { filesRoute } from "../lib/files";
import { formatBytes } from "../lib/format";
import { STATUS_LABEL, documentTitle, isPdf, type LibraryItem, type ReadingStatus } from "../lib/library";
import { EpubReader } from "./EpubReader";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";
const STATUS_TONE: Record<ReadingStatus | "unread", string> = {
  reading: "text-accent",
  want: "text-ink",
  finished: "text-ok",
  unread: "text-muted",
};

/** Open a PDF in the browser's viewer in a new tab (from the saved copy when there is one). */
async function openPdf(fileId: string) {
  const tab = window.open("", "_blank");
  try {
    const blob = await documentBytes(fileId);
    const url = URL.createObjectURL(new Blob([blob], { type: "application/pdf" }));
    if (tab) tab.location.href = url;
    else window.location.href = url;
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
  } catch (e) {
    tab?.close();
    throw e;
  }
}

export function Library({ online }: ModuleProps) {
  const library = useLibrary(online);
  const { list } = library;
  const [editing, setEditing] = useState<string>();
  const [reading, setReading] = useState<string>();
  const offline = list.stale || !online;
  const items = list.items;
  const current = items?.filter((i) => i.progress?.status === "reading") ?? [];
  const open = items?.find((i) => i.file.id === reading);

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      {open && (
        <EpubReader
          key={open.file.id}
          file={open.file}
          progress={open.progress}
          onClose={() => setReading(undefined)}
          onProgress={(patch) => library.setProgress(open.file.id, patch)}
        />
      )}
      <Panel title={`library${items ? ` · ${items.length}` : ""}`}>
        <p className="mb-3 text-sm text-muted">
          PDFs and ePubs from your <a href={filesRoute("")} className="text-accent underline-offset-4 hover:underline">Files</a>. Track
          where you are in each one, and save books to this device to read offline.
        </p>
        {offline && items && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: books saved on this device can be read; progress is saved here and syncs when you reconnect.
          </p>
        )}
        {list.error && !offline && (
          <p role="alert" className="mb-3 text-sm text-bad">
            {list.error}
          </p>
        )}
        {list.loading && !items ? (
          <p className="text-sm text-muted">Loading…</p>
        ) : !items ? (
          <div className="text-sm">
            <p className="text-muted">The library couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={list.reload}>
              retry
            </Button>
          </div>
        ) : items.length === 0 ? (
          <p className="text-sm text-muted">No documents yet. Upload a PDF or ePub in Files and it appears here.</p>
        ) : (
          <>
            {current.length > 0 && <p className="label mb-2">currently reading · {current.length}</p>}
            <ul className="divide-y divide-line">
              {items.map((item) => (
                <DocumentRow
                  key={item.file.id}
                  item={item}
                  library={library}
                  offline={offline}
                  editing={editing === item.file.id}
                  onEdit={(on) => setEditing(on ? item.file.id : undefined)}
                  onRead={() => setReading(item.file.id)}
                />
              ))}
            </ul>
          </>
        )}
      </Panel>
    </div>
  );
}

function DocumentRow(props: {
  item: LibraryItem;
  library: LibraryState;
  offline: boolean;
  editing: boolean;
  onEdit: (on: boolean) => void;
  /** Open an ePub in the reader. */
  onRead: () => void;
}) {
  const { file, progress } = props.item;
  const { library } = props;
  const status = progress?.status ?? "unread";
  const savedBytes = library.saved.get(file.id);
  const available = !props.offline || savedBytes !== undefined;
  const [page, setPage] = useState(String(progress?.current_page ?? ""));
  const [total, setTotal] = useState(String(progress?.total_pages ?? ""));
  const [busy, setBusy] = useState(false);
  const fail = (m: string | undefined) => library.list.setError(m);

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
      props.onEdit(false);
    } finally {
      setBusy(false);
    }
  };

  const save = (e: FormEvent) => {
    e.preventDefault();
    const patch: { current_page?: number; total_pages?: number } = {};
    if (total.trim()) patch.total_pages = Number(total);
    if (page.trim()) patch.current_page = Number(page);
    void act(() => library.setProgress(file.id, patch));
  };

  return (
    <li className="py-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm">{documentTitle(file)}</span>
          <span className="font-mono text-[11px] text-muted">
            <span className={STATUS_TONE[status]}>{STATUS_LABEL[status]}</span>
            {progress?.current_page != null && progress.total_pages != null && ` · ${isPdf(file) ? "p." : "ch."} ${progress.current_page} of ${progress.total_pages}`}
            {` · ${isPdf(file) ? "PDF" : "ePub"}`}
            {savedBytes !== undefined && ` · saved offline (${formatBytes(savedBytes)})`}
          </span>
        </span>
        <span className="flex flex-wrap gap-1">
          {isPdf(file) ? (
            <Button className={small} disabled={!available} onClick={() => void openPdf(file.id).catch((e: Error) => fail(e.message))}>
              open
            </Button>
          ) : (
            <Button className={small} variant="primary" disabled={!available} onClick={props.onRead}>
              read
            </Button>
          )}
          <Button
            className={small}
            disabled={busy || (props.offline && savedBytes === undefined)}
            aria-label={savedBytes !== undefined ? `Remove “${documentTitle(file)}” from this device` : `Save “${documentTitle(file)}” for offline`}
            onClick={() =>
              void act(async () => {
                if (savedBytes !== undefined) await library.unsave(file.id);
                else fail(await library.save(file.id));
              })
            }
          >
            {savedBytes !== undefined ? "remove download" : "download"}
          </Button>
          <Button className={small} onClick={() => props.onEdit(!props.editing)}>
            progress
          </Button>
        </span>
      </div>
      {progress && progress.percent > 0 && (
        <div className="mt-2 h-1 overflow-hidden rounded-full bg-sunken" role="progressbar" aria-valuenow={progress.percent} aria-valuemin={0} aria-valuemax={100} aria-label={`${documentTitle(file)} progress`}>
          <div className={`h-full ${status === "finished" ? "bg-ok" : "bg-accent"}`} style={{ width: `${progress.percent}%` }} />
        </div>
      )}
      {props.editing && (
        <div className="mt-3 flex flex-col gap-2 rounded-sm border border-line bg-sunken/40 p-3">
          <form onSubmit={save} className="flex flex-wrap items-center gap-2 text-sm">
            <span>Page</span>
            <input value={page} onChange={(e) => setPage(e.target.value.replace(/\D/g, ""))} inputMode="numeric" aria-label="Current page" className={`${inputClass} w-20`} />
            <span>of</span>
            <input value={total} onChange={(e) => setTotal(e.target.value.replace(/\D/g, ""))} inputMode="numeric" aria-label="Total pages" className={`${inputClass} w-20`} />
            <Button type="submit" variant="primary" className={small} disabled={busy}>
              save
            </Button>
          </form>
          <div className="flex flex-wrap gap-1.5">
            {(["want", "reading", "finished"] as const).map((s) => (
              <Button key={s} className={small} disabled={busy || status === s} onClick={() => void act(() => library.setProgress(file.id, { status: s }))}>
                {STATUS_LABEL[s].toLowerCase()}
              </Button>
            ))}
            {progress && (
              <Button
                variant="danger"
                className={small}
                disabled={busy || props.offline}
                onClick={() =>
                  void act(() =>
                    library.list
                      .mutate(
                        async () => {
                          await api.clearProgress(file.id);
                          return true;
                        },
                        (cur) => cur.map((i) => (i.file.id === file.id ? { ...i, progress: null } : i)),
                      )
                      .catch(reportIfUnauthorized),
                  )
                }
              >
                reset
              </Button>
            )}
          </div>
        </div>
      )}
    </li>
  );
}
