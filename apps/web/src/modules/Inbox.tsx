import { useState } from "react";
import type { ModuleProps } from "../App";
import { CaptureBox } from "../components/CaptureBox";
import { Button, Panel } from "../components/ui";
import { useInbox } from "../hooks/useInbox";
import { formatDate, formatTime } from "../lib/format";
import type { Triaged } from "../lib/inbox";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

function describe(t: Triaged): { text: string; route: string } {
  switch (t.kind) {
    case "task":
      return { text: `Added task “${t.task.title}”.`, route: "/tasks" };
    case "note":
      return { text: "Saved as a note.", route: "/notes" };
    case "journal":
      return { text: `Added to the journal for ${t.entry.date}.`, route: "/journal" };
  }
}

export function Inbox({ online }: ModuleProps) {
  const inbox = useInbox(online);
  const [busy, setBusy] = useState<string>();
  const [result, setResult] = useState<{ text: string; route: string }>();
  // Capture and discard work offline (queued); triage needs the server.
  const offline = inbox.stale || !online;

  const triage = async (id: string, to: "task" | "note" | "journal") => {
    setBusy(id);
    const done = await inbox.triage(id, to);
    setBusy(undefined);
    if (done) setResult(describe(done));
  };

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <Panel title={`inbox${inbox.items ? ` · ${inbox.items.length}` : ""}`}>
        <CaptureBox disabled={!inbox.items} onCapture={(t) => inbox.capture(t)} />
        {result && (
          <p role="status" className="mt-3 text-sm text-muted">
            {result.text}{" "}
            <a href={`#${result.route}`} className="text-accent underline underline-offset-4">
              open
            </a>
          </p>
        )}
        {inbox.error && !offline && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {inbox.error}
          </p>
        )}
        {offline && inbox.items && (
          <p className="mt-3 text-sm text-muted">Offline: captures are saved on this device and sync when you reconnect. Sorting needs a connection.</p>
        )}

        {inbox.loading && !inbox.items ? (
          <p className="mt-4 text-sm text-muted">Loading…</p>
        ) : !inbox.items ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">The inbox couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={inbox.reload}>
              retry
            </Button>
          </div>
        ) : inbox.items.length === 0 ? (
          <p className="mt-4 text-sm text-muted">Inbox zero. Capture anything above, and sort it later.</p>
        ) : (
          <ul className="mt-4 divide-y divide-line">
            {inbox.items.map((item) => (
              <li key={item.id} className="py-3">
                <p className="text-sm whitespace-pre-wrap">{item.text}</p>
                <div className="mt-2 flex flex-wrap items-center gap-1.5">
                  <span className="mr-auto font-mono text-[11px] text-muted">
                    {formatDate(item.created_at)} {formatTime(item.created_at)}
                  </span>
                  <Button className={small} disabled={offline || busy === item.id} onClick={() => void triage(item.id, "task")}>
                    → task
                  </Button>
                  <Button className={small} disabled={offline || busy === item.id} onClick={() => void triage(item.id, "note")}>
                    → note
                  </Button>
                  <Button className={small} disabled={offline || busy === item.id} onClick={() => void triage(item.id, "journal")}>
                    → journal
                  </Button>
                  <Button
                    variant="danger"
                    className={small}
                    disabled={busy === item.id}
                    aria-label={`Discard “${item.text.slice(0, 40)}”`}
                    onClick={() => void inbox.remove(item.id)}
                  >
                    discard
                  </Button>
                </div>
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}
