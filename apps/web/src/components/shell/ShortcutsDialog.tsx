import { useEffect, useRef, useState } from "react";
import { SHORTCUTS, keyLabel, searchShortcuts, type Shortcut } from "../../lib/shortcuts";
import { Icon } from "../icons";
import { inputClass } from "../ui";

const GROUPS: Shortcut["group"][] = ["Navigate", "Capture", "Panels", "Help"];

export function Keys({ s }: { s: Shortcut }) {
  return (
    <span className="flex shrink-0 items-center gap-1">
      {s.keys.map((k, i) => (
        <span key={i} className="flex items-center gap-1">
          {i > 0 && <span className="text-xs text-muted">{s.sequence ? "then" : "+"}</span>}
          <kbd className="min-w-6 rounded-[5px] border border-line bg-sunken px-1.5 py-0.5 text-center font-mono text-xs">{keyLabel(k)}</kbd>
        </span>
      ))}
    </span>
  );
}

/** Searchable help: every shortcut, grouped, with the platform's key names. */
export function ShortcutsDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [q, setQ] = useState("");

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) {
      setQ("");
      d.showModal?.();
    }
    if (!open && d.open) d.close();
  }, [open]);

  const found = searchShortcuts(q, SHORTCUTS);

  return (
    <dialog
      ref={dialog}
      onClose={onClose}
      aria-labelledby="shortcuts-title"
      className="m-auto max-h-[min(40rem,calc(100dvh-2rem))] w-[min(34rem,calc(100vw-2rem))] rounded-lg border border-line bg-panel p-0 text-ink shadow-lift backdrop:bg-black/40"
    >
      <div className="flex items-center gap-3 border-b border-line px-5 py-4">
        <Icon name="keyboard" className="text-accent" />
        <h2 id="shortcuts-title" className="flex-1 text-[17px] font-medium">
          Keyboard shortcuts
        </h2>
        <button type="button" onClick={onClose} aria-label="Close" className="grid size-9 place-items-center rounded-full text-muted hover:bg-sunken hover:text-ink">
          <Icon name="close" />
        </button>
      </div>
      <div className="px-5 pt-4">
        <input
          type="search"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Find a shortcut"
          aria-label="Find a shortcut"
          className={`${inputClass} w-full`}
          autoFocus
        />
      </div>
      <div className="px-5 pt-2 pb-5">
        {found.length === 0 && <p className="py-6 text-center text-sm text-muted">No shortcut matches “{q}”.</p>}
        {GROUPS.map((g) => {
          const items = found.filter((s) => s.group === g);
          if (!items.length) return null;
          return (
            <section key={g} className="mt-4">
              <h3 className="text-sm font-medium text-muted">{g}</h3>
              <ul className="mt-1 divide-y divide-line">
                {items.map((s) => (
                  <li key={s.id} className="flex min-h-10 items-center justify-between gap-4 text-sm">
                    <span>{s.label}</span>
                    <Keys s={s} />
                  </li>
                ))}
              </ul>
            </section>
          );
        })}
        <p className="mt-5 text-xs text-muted">Shortcuts pause while you type in a field. Press Esc to close.</p>
      </div>
    </dialog>
  );
}
