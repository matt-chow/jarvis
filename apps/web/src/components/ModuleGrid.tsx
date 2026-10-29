import type { ModuleProps } from "../App";
import { MODULES, type ModuleId } from "../lib/modules";
import { StageTag } from "./ui";

/** Every module as a card: the Home "modules" widget and the phone's Modules screen. */
export function ModuleGrid({ navigate, exclude = [] }: { navigate: ModuleProps["navigate"]; exclude?: readonly ModuleId[] }) {
  return (
    // Two compact columns on a phone; the summary appears from `sm` up.
    <ul className="grid grid-cols-2 gap-3 lg:grid-cols-3">
      {MODULES.filter((m) => !exclude.includes(m.id)).map((m) => (
        <li key={m.id}>
          <button
            type="button"
            onClick={() => navigate(m.id)}
            className="group flex h-full w-full flex-col rounded-lg border border-line bg-panel p-4 text-left transition hover:border-accent focus-visible:border-accent sm:p-5"
          >
            <span className="flex items-center justify-between">
              <span aria-hidden className="grid size-9 place-items-center rounded-sm bg-sunken font-mono text-lg text-accent">
                {m.glyph}
              </span>
              {m.stage !== "ready" && <StageTag stage={m.stage} />}
            </span>
            <span className="mt-4 text-[15px] font-medium">{m.label}</span>
            <span className="mt-1 hidden text-sm text-muted sm:block">{m.summary}</span>
            <span className="label mt-auto pt-3">{m.offline ? "works offline" : "needs connection"}</span>
          </button>
        </li>
      ))}
    </ul>
  );
}
