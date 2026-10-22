import { useCallback, useEffect, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { describeError } from "../hooks/useCachedList";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { filesRoute } from "../lib/files";
import { STATUS_LABEL, documentTitle, isPdf, type LibraryItem, type ReadingStatus } from "../lib/library";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";
const STATUS_TONE: Record<ReadingStatus | "unread", string> = {
  reading: "text-accent",
  want: "text-ink",
  finished: "text-ok",
  unread: "text-muted",
};

/** Open a PDF in a new tab via a blob URL (the API serves files as attachments). */
async function openPdf(fileId: string) {
  const tab = window.open("", "_blank");
  const res = await fetch(api.fileContentUrl(fileId), { credentials: "same-origin" });
  if (!res.ok) {
    tab?.close();
    throw new Error("Couldn't open the document.");
  }
  const url = URL.createObjectURL(new Blob([await res.blob()], { type: "application/pdf" }));
  if (tab) tab.location.href = url;
  else window.location.href = url;
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

export function Library({ online }: ModuleProps) {
  const [items, setItems] = useState<LibraryItem[]>();
  const [error, setError] = useState<string>();
  const [editing, setEditing] = useState<string>();

  const reload = useCallback(async () => {
    try {
      setItems(await api.library());
      setError(undefined);
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describeError(e, "document"));
    }
  }, []);

  useEffect(() => {
    if (online) void reload();
  }, [online, reload]);

  if (!online) {
    return (
      <Panel title="library">
        <p className="text-sm text-muted">The library needs a connection.</p>
      </Panel>
    );
  }

  const reading = items?.filter((i) => i.progress?.status === "reading") ?? [];
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <Panel title={`library${items ? ` · ${items.length}` : ""}`}>
        <p className="mb-3 text-sm text-muted">
          PDFs and ePubs from your <a href={filesRoute("")} className="text-accent underline-offset-4 hover:underline">Files</a>. Track
          where you are in each one.
        </p>
        {error && (
          <p role="alert" className="mb-3 text-sm text-bad">
            {error}
          </p>
        )}
        {!items ? (
          <p className="text-sm text-muted">Loading…</p>
        ) : items.length === 0 ? (
          <p className="text-sm text-muted">No documents yet. Upload a PDF or ePub in Files and it appears here.</p>
        ) : (
          <>
            {reading.length > 0 && <p className="label mb-2">currently reading · {reading.length}</p>}
            <ul className="divide-y divide-line">
              {items.map((item) => (
                <DocumentRow
                  key={item.file.id}
                  item={item}
                  editing={editing === item.file.id}
                  onEdit={(on) => setEditing(on ? item.file.id : undefined)}
                  onChanged={reload}
                  onError={setError}
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
  editing: boolean;
  onEdit: (on: boolean) => void;
  onChanged: () => Promise<void>;
  onError: (m: string) => void;
}) {
  const { file, progress } = props.item;
  const status = progress?.status ?? "unread";
  const [page, setPage] = useState(String(progress?.current_page ?? ""));
  const [total, setTotal] = useState(String(progress?.total_pages ?? ""));
  const [busy, setBusy] = useState(false);

  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await fn();
      await props.onChanged();
      props.onEdit(false);
    } catch (e) {
      reportIfUnauthorized(e);
      props.onError(describeError(e, "document"));
    } finally {
      setBusy(false);
    }
  };

  const save = (e: FormEvent) => {
    e.preventDefault();
    const patch: { current_page?: number; total_pages?: number } = {};
    if (total.trim()) patch.total_pages = Number(total);
    if (page.trim()) patch.current_page = Number(page);
    void act(() => api.setProgress(file.id, patch));
  };

  return (
    <li className="py-3">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm">{documentTitle(file)}</span>
          <span className="font-mono text-[11px] text-muted">
            <span className={STATUS_TONE[status]}>{STATUS_LABEL[status]}</span>
            {progress?.current_page != null && progress.total_pages != null && ` · p. ${progress.current_page} of ${progress.total_pages}`}
            {` · ${isPdf(file) ? "PDF" : "ePub"}`}
          </span>
        </span>
        <span className="flex gap-1">
          {isPdf(file) ? (
            <Button className={small} onClick={() => void openPdf(file.id).catch((e: Error) => props.onError(e.message))}>
              open
            </Button>
          ) : (
            <a href={api.fileContentUrl(file.id)} download={file.name} className="inline-flex min-h-7 items-center rounded-sm border border-line px-2 font-mono text-[10px] uppercase hover:bg-sunken">
              download
            </a>
          )}
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
              <Button key={s} className={small} disabled={busy || status === s} onClick={() => void act(() => api.setProgress(file.id, { status: s }))}>
                {STATUS_LABEL[s].toLowerCase()}
              </Button>
            ))}
            {progress && (
              <Button variant="danger" className={small} disabled={busy} onClick={() => void act(() => api.clearProgress(file.id))}>
                reset
              </Button>
            )}
          </div>
        </div>
      )}
    </li>
  );
}
