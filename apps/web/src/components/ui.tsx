import type { ReactNode } from "react";

// Shared components from the Paper style board (D-046): sheet cards with a
// mono eyebrow, pill buttons and chips. Every screen builds on these.

export type Tone = "ok" | "warn" | "bad" | "idle" | "accent";

const toneText: Record<Tone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  bad: "text-bad",
  idle: "text-muted",
  accent: "text-accent",
};

/** Compact status pill: `● LABEL value`. */
export function StatusChip({ tone, label, value, title }: { tone: Tone; label: string; value: string; title?: string }) {
  return (
    <span
      title={title}
      className="inline-flex items-center gap-1.5 rounded-full border border-line bg-panel px-2.5 py-1 font-mono text-[11px] leading-none"
    >
      <span aria-hidden className={`${toneText[tone]} text-[8px]`}>
        ●
      </span>
      <span className="tracking-wider text-muted uppercase">{label}</span>
      <span className={toneText[tone]}>{value}</span>
    </span>
  );
}

/** Sheet card: a mono eyebrow title, optional actions, and content. */
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
  return (
    <section className={`overflow-hidden rounded-lg border border-line bg-panel ${className}`}>
      <header className="flex min-h-12 items-center gap-3 px-5 pt-4">
        <h2 className="label flex-1 truncate">{title}</h2>
        {actions}
      </header>
      <div className="px-5 pt-3 pb-5">{children}</div>
    </section>
  );
}

/** Pill button. Labels render in sentence case (the source strings stay as written). */
export function Button({
  children,
  variant = "default",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "default" | "primary" | "danger" }) {
  const styles = {
    default: "border-line bg-panel text-ink hover:bg-sunken disabled:opacity-50",
    // Disabled primary reads as an empty pill, not a faded ink one.
    primary: "border-ink bg-ink text-canvas hover:opacity-90 disabled:border-line disabled:bg-transparent disabled:text-muted disabled:hover:opacity-100",
    danger: "border-line bg-panel text-bad hover:bg-sunken disabled:opacity-50",
  }[variant];
  return (
    <button
      type="button"
      {...props}
      className={`inline-flex min-h-9 items-center justify-center gap-2 rounded-full border px-4 text-[13px] font-medium transition active:scale-[0.98] disabled:cursor-not-allowed disabled:active:scale-100 ${styles} ${props.className ?? ""}`}
    >
      <span className="inline-block first-letter:uppercase">{children}</span>
    </button>
  );
}

export function StageTag({ stage }: { stage: "ready" | "preview" | "planned" }) {
  const tone = { ready: "text-ok", preview: "text-accent", planned: "text-muted" }[stage];
  return <span className={`font-mono text-[10px] tracking-widest uppercase ${tone}`}>{stage}</span>;
}

/** Shared text input / select / textarea styling. */
export const inputClass =
  "min-h-10 rounded-sm border border-line bg-panel px-3 text-[15px] text-ink placeholder:text-muted focus:border-accent disabled:opacity-60";
