import type { ReactNode } from "react";

export type Tone = "ok" | "warn" | "bad" | "idle" | "accent";

const toneText: Record<Tone, string> = {
  ok: "text-ok",
  warn: "text-warn",
  bad: "text-bad",
  idle: "text-muted",
  accent: "text-accent",
};

/** Compact status indicator: `● LABEL value`. */
export function StatusChip({ tone, label, value, title }: { tone: Tone; label: string; value: string; title?: string }) {
  return (
    <span
      title={title}
      className="inline-flex items-center gap-1.5 rounded-sm border border-line bg-panel px-2 py-1 font-mono text-[11px] leading-none"
    >
      <span aria-hidden className={`${toneText[tone]} text-[9px]`}>
        ●
      </span>
      <span className="tracking-wider text-muted uppercase">{label}</span>
      <span className={toneText[tone]}>{value}</span>
    </span>
  );
}

/** Window-style card with a retro title bar. */
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
    <section className={`overflow-hidden rounded-md border border-line bg-panel shadow-[0_1px_0_var(--line)] ${className}`}>
      <header className="flex items-center gap-3 border-b border-line bg-sunken/60 px-3 py-2">
        <span aria-hidden className="flex gap-1">
          <span className="size-2 rounded-[1px] bg-line" />
          <span className="size-2 rounded-[1px] bg-line" />
          <span className="size-2 rounded-[1px] bg-line" />
        </span>
        <h2 className="label flex-1 truncate">{title}</h2>
        {actions}
      </header>
      <div className="p-4">{children}</div>
    </section>
  );
}

export function Button({
  children,
  variant = "default",
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "default" | "primary" | "danger" }) {
  const styles = {
    default: "border-line bg-panel hover:bg-sunken text-ink",
    primary: "border-accent bg-accent text-accent-ink hover:brightness-110",
    danger: "border-line bg-panel text-bad hover:bg-sunken",
  }[variant];
  return (
    <button
      type="button"
      {...props}
      className={`inline-flex min-h-9 items-center justify-center gap-2 rounded-sm border px-3 font-mono text-xs tracking-wide uppercase transition disabled:cursor-not-allowed disabled:opacity-50 ${styles} ${props.className ?? ""}`}
    >
      {children}
    </button>
  );
}

export function StageTag({ stage }: { stage: "ready" | "preview" | "planned" }) {
  const tone = { ready: "text-ok", preview: "text-accent", planned: "text-muted" }[stage];
  return <span className={`font-mono text-[10px] tracking-widest uppercase ${tone}`}>[{stage}]</span>;
}
