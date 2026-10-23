import { useEffect, useMemo, useRef, useState } from "react";
import { Button, Panel } from "../components/ui";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { chapterDocument, parseBook, unzip, type Book } from "../lib/epub";
import type { FileEntry } from "../lib/files";
import type { Progress } from "../lib/library";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";
const SAVE_DELAY_MS = 600;

/**
 * Reads an ePub in the app. Chapters render in a sandboxed iframe (no
 * scripts, opaque origin); moving between chapters saves reading progress
 * (chapter n of N).
 */
export function EpubReader({ file, progress, onClose, onSaved }: { file: FileEntry; progress: Progress | null; onClose: () => void; onSaved: () => void }) {
  const [book, setBook] = useState<Book>();
  const [error, setError] = useState<string>();
  const [index, setIndex] = useState(0);
  const urls = useRef(new Map<string, string>());
  const first = useRef(true);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(api.fileContentUrl(file.id), { credentials: "same-origin" });
        if (!res.ok) throw new Error(`Couldn't load the book (${res.status}).`);
        const b = parseBook(await unzip(await res.arrayBuffer()));
        if (cancelled) return;
        setBook(b);
        // Resume where the saved progress says, if it was saved for this book.
        const saved = progress?.total_pages === b.chapters.length ? (progress.current_page ?? 1) - 1 : 0;
        setIndex(Math.min(Math.max(saved, 0), b.chapters.length - 1));
      } catch (e) {
        reportIfUnauthorized(e);
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    const made = urls.current;
    return () => {
      cancelled = true;
      for (const u of made.values()) URL.revokeObjectURL(u);
      made.clear();
    };
    // Load once per file: `progress` only seeds the starting chapter.
  }, [file.id]);

  const doc = useMemo(() => {
    if (!book) return "";
    return chapterDocument(book, index, (path) => {
      let url = urls.current.get(path);
      const data = book.zip.get(path);
      if (!url && data) {
        url = URL.createObjectURL(new Blob([data as BlobPart], { type: book.types.get(path) }));
        urls.current.set(path, url);
      }
      return url;
    });
  }, [book, index]);

  // Save progress after the reader settles on a chapter (not on first open).
  useEffect(() => {
    if (!book) return;
    if (first.current) {
      first.current = false;
      return;
    }
    const t = setTimeout(() => {
      api
        .setProgress(file.id, { current_page: index + 1, total_pages: book.chapters.length, position: `epub:${book.chapters[index].path}`.slice(0, 200) })
        .then(onSaved, (e: unknown) => {
          reportIfUnauthorized(e);
          setError("Couldn't save your place.");
        });
    }, SAVE_DELAY_MS);
    return () => clearTimeout(t);
  }, [book, index, file.id, onSaved]);

  const go = (by: number) => book && setIndex((i) => Math.min(Math.max(i + by, 0), book.chapters.length - 1));

  return (
    <Panel
      title={book?.title ?? "reader"}
      actions={
        <Button className={small} onClick={onClose} aria-label="Close the reader">
          close
        </Button>
      }
    >
      {error ? (
        <p role="alert" className="text-sm text-bad">
          {error}
        </p>
      ) : !book ? (
        <p className="text-sm text-muted">Opening the book…</p>
      ) : (
        <div
          onKeyDown={(e) => {
            if (e.key === "ArrowRight") go(1);
            if (e.key === "ArrowLeft") go(-1);
          }}
        >
          <div className="mb-2 flex items-center justify-between gap-2">
            <Button className={small} aria-label="Previous chapter" disabled={index === 0} onClick={() => go(-1)}>
              ‹ previous
            </Button>
            <span role="status" className="font-mono text-[11px] text-muted">
              Chapter {index + 1} of {book.chapters.length}
            </span>
            <Button className={small} aria-label="Next chapter" disabled={index >= book.chapters.length - 1} onClick={() => go(1)}>
              next ›
            </Button>
          </div>
          <iframe
            title={`${book.title}, chapter ${index + 1}`}
            sandbox=""
            srcDoc={doc}
            className="h-[70vh] w-full rounded-sm border border-line bg-panel"
          />
        </div>
      )}
    </Panel>
  );
}
