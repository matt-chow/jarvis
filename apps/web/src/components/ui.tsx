import type { ReactNode } from "react";

// Shared components (Paper style board D-046, redesigned D-050): sheet cards
// with a plain heading, pill buttons and chips. Every screen builds on these.

export type Tone = "ok" | "warn" | "bad" | "idle" | "accent";

const toneText: Record<Tone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  bad: "text-bad",
  idle: "text-muted",
  accent: "text-accent",
};

const toneDot: Record<Tone, string> = { ok: "bg-ok", warn: "bg-warn", bad: "bg-bad", idle: "bg-muted", accent: "bg-accent" };

/** Compact status pill: a dot, a label, and a value. */
export function StatusChip({ tone, label, value, title }: { tone: Tone; label: string; value: string; title?: string }) {
  return (
    <span title={title} className="inline-flex items-center gap-1.5 rounded-full border border-line bg-panel px-2.5 py-1 text-xs leading-none">
      <span aria-hidden className={`size-1.5 rounded-full ${toneDot[tone]}`} />
      <span className="text-muted first-letter:uppercase">{label}</span>
      <span className={`font-mono ${toneText[tone]}`}>{value}</span>
    </span>
  );
}

/**
 * Sheet card with a heading. A title like `"habits · 2/5 today"` renders as
 * "Habits" with the count beside it in the data face.
 */
export function Panel({
  title,
  actions,
  children,
  className = "",
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  const [head, ...rest] = title.split(" · ");
  return (
    <section className={`overflow-hidden rounded-lg border border-line bg-panel ${className}`}>
      <header className="flex min-h-12 flex-wrap items-center gap-x-3 gap-y-2 px-[var(--pad-card)] pt-4">
        <h2 className="flex min-w-[min(100%,10rem)] flex-1 items-baseline gap-2 text-[15px] font-medium">
          <span className="truncate first-letter:uppercase">{head}</span>
          {rest.length > 0 && <span className="truncate font-mono text-xs font-normal text-muted">{rest.join(" · ")}</span>}
        </h2>
        {actions}
      </header>
      <div className="px-[var(--pad-card)] pt-3 pb-[var(--pad-card)]">{children}</div>
    </section>
  );
}

/** Pill button. Labels render in sentence case (the source strings stay as written). */
export function Button({
  children,
  variant = "default",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "default" | "primary" | "danger" | "ghost" }) {
  const styles = {
    default: "border-line bg-panel text-ink hover:bg-sunken disabled:opacity-50",
    // Disabled primary reads as an empty pill, not a faded accent one.
    primary: "border-accent bg-accent text-accent-ink hover:opacity-90 disabled:border-line disabled:bg-transparent disabled:text-muted disabled:hover:opacity-100",
    danger: "border-line bg-panel text-bad hover:bg-sunken disabled:opacity-50",
    ghost: "border-transparent bg-transparent text-muted hover:bg-sunken hover:text-ink disabled:opacity-50",
  }[variant];
  return (
    <button
      type="button"
      {...props}
      className={`press inline-flex min-h-9 items-center justify-center gap-2 rounded-full border px-4 text-[13px] font-medium transition-colors disabled:cursor-not-allowed ${styles} ${props.className ?? ""}`}
    >
      <span className="inline-flex items-center gap-2 first-letter:uppercase">{children}</span>
    </button>
  );
}

export function StageTag({ stage }: { stage: "ready" | "preview" | "planned" }) {
  const tone = { ready: "text-ok", preview: "text-accent", planned: "text-muted" }[stage];
  return <span className={`font-mono text-[10px] tracking-widest uppercase ${tone}`}>{stage}</span>;
}

/** An honest empty state: what this place is for and the one next step. */
export function Empty({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-2 rounded-md border border-dashed border-line px-4 py-5">
      <p className="text-[15px] font-medium">{title}</p>
      {children && <p className="max-w-prose text-sm text-muted">{children}</p>}
      {action && <div className="mt-1">{action}</div>}
    </div>
  );
}

/** Shared text input / select / textarea styling. */
export const inputClass =
  "min-h-10 rounded-sm border border-line bg-panel px-3 text-[15px] text-ink placeholder:text-muted focus:border-accent disabled:opacity-60";
