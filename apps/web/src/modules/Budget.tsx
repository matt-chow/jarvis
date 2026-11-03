import { useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import type { ModuleProps } from "../App";
import { Button, Empty, Panel, StatusChip, inputClass } from "../components/ui";
import { useBudget, type BudgetState } from "../hooks/useBudget";
import { useHashParam } from "../hooks/useHashParam";
import {
  ACCOUNT_KINDS,
  CADENCES,
  MAX_TRANSACTIONS,
  RANGE_PRESETS,
  balances,
  budgetLines,
  byCategory,
  checkTransaction,
  formatMoney,
  monthOf,
  monthlyTotals,
  netWorth,
  parseMoney,
  presetRange,
  previewImport,
  sortTransactions,
  toCsv,
  toDecimal,
  totals,
  upcoming,
  type Account,
  type AccountKind,
  type BudgetCategory,
  type BudgetData,
  type Cadence,
  type Flow,
  type ImportRow,
  type RangePreset,
  type Template,
  type TemplateInput,
  type Transaction,
  type TransactionInput,
} from "../lib/budget";
import { PALETTE } from "../lib/categories";
import { clientId } from "../lib/sync";
import { localDate } from "../lib/tasks";

// Budget (D-050): records on the server, figures derived on the device.
// No bank connections; nothing here is financial advice.

type View = "overview" | "transactions" | "accounts" | "budgets" | "recurring" | "files";
const VIEWS_: { id: View; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "transactions", label: "Transactions" },
  { id: "accounts", label: "Accounts" },
  { id: "budgets", label: "Categories" },
  { id: "recurring", label: "Recurring" },
  { id: "files", label: "Import and export" },
];

const small = "min-h-8 px-3 text-xs";
const FLOW_LABEL: Record<Flow, string> = { expense: "Spending", income: "Income", transfer: "Transfer" };

export function Budget({ online }: ModuleProps) {
  const state = useBudget(online);
  const [view, setView] = useState<View>("overview");
  const [adding, setAdding] = useState<Flow | null>(null);
  const today = localDate();
  const offline = state.stale || !online;

  // Alt+M and "#/budget?add=expense" open the add form.
  const add = useHashParam("add");
  useEffect(() => {
    if (add === "expense" || add === "income" || add === "transfer") {
      setAdding(add);
      // Replace (fires hashchange) so the next Alt+M opens the form again.
      location.replace("#/budget");
    }
  }, [add]);

  const data = state.data;
  if (!data) {
    return state.loading ? (
      <p className="text-sm text-muted">Loading your budget…</p>
    ) : (
      <Panel title="budget">
        <p className="text-sm text-muted">{state.error ?? "The budget couldn't be loaded."}</p>
        <Button className={`${small} mt-3`} onClick={state.reload}>
          Try again
        </Button>
      </Panel>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      {offline && (
        <p className="rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
          Offline: changes are saved on this device and sync when you reconnect.
        </p>
      )}
      {state.error && !offline && (
        <p role="alert" className="text-sm text-bad">
          {state.error}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <div role="tablist" aria-label="Budget sections" className="-mx-1 flex max-w-full gap-1 overflow-x-auto px-1">
          {VIEWS_.map((v) => (
            <button
              key={v.id}
              role="tab"
              type="button"
              aria-selected={view === v.id}
              onClick={() => setView(v.id)}
              className={`press min-h-9 shrink-0 rounded-full border px-3 text-[13px] ${view === v.id ? "border-ink bg-ink text-canvas" : "border-line bg-panel text-muted hover:text-ink"}`}
            >
              {v.label}
            </button>
          ))}
        </div>
        {data.accounts.length > 0 && (
          <Button variant="primary" className="ml-auto" onClick={() => setAdding("expense")}>
            Add transaction
          </Button>
        )}
      </div>

      {adding && data.accounts.length > 0 && (
        <Panel title="new transaction">
          <TransactionForm
            data={data}
            kind={adding}
            today={today}
            onCancel={() => setAdding(null)}
            onSave={async (input) => {
              const ok = (await state.save("transactions", txnInput(input))) !== undefined;
              if (ok) setAdding(null);
              return ok;
            }}
          />
        </Panel>
      )}

      {data.accounts.length === 0 ? (
        <Panel title="start with an account">
          <p className="mb-3 max-w-prose text-sm text-muted">
            Add the accounts you want to track: chequing, savings, a credit card, or cash. Enter what's in each today as its opening balance. Nothing connects to a bank; you record or import transactions yourself.
          </p>
          <AccountForm data={data} onSave={async (input) => (await state.save("accounts", input)) !== undefined} />
        </Panel>
      ) : (
        <div role="tabpanel" aria-label={VIEWS_.find((v) => v.id === view)?.label}>
          {view === "overview" && <Overview data={data} state={state} today={today} onOpen={setView} />}
          {view === "transactions" && <Transactions data={data} state={state} today={today} />}
          {view === "accounts" && <Accounts data={data} state={state} />}
          {view === "budgets" && <Categories data={data} state={state} today={today} />}
          {view === "recurring" && <Recurring data={data} state={state} today={today} />}
          {view === "files" && <Files data={data} state={state} today={today} />}
        </div>
      )}
    </div>
  );
}

// ---- Overview ----

function RangePicker({ value, onChange }: { value: RangePreset; onChange: (r: RangePreset) => void }) {
  return (
    <label className="flex items-center gap-2 text-sm text-muted">
      <span>Period</span>
      <select className={`${inputClass} min-h-9 text-sm`} value={value} onChange={(e) => onChange(e.target.value as RangePreset)}>
        {RANGE_PRESETS.map((p) => (
          <option key={p.id} value={p.id}>
            {p.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function Stat({ label, value, tone }: { label: string; value: string; tone?: "ok" | "bad" }) {
  return (
    <div className="flex min-w-0 flex-col gap-1 rounded-md border border-line px-3 py-2.5">
      <span className="text-xs text-muted">{label}</span>
      <span className={`truncate font-mono text-lg ${tone === "bad" ? "text-bad" : tone === "ok" ? "text-ok" : ""}`}>{value}</span>
    </div>
  );
}

/** A chart with its data as a table (collapsed by default; the same numbers). */
function Figure({ label, chart, table }: { label: string; chart: ReactNode; table: ReactNode }) {
  return (
    <figure className="flex flex-col gap-2">
      <div role="img" aria-label={label}>
        {chart}
      </div>
      <details className="text-sm">
        <summary className="cursor-pointer text-xs text-muted hover:text-ink">Show as a table</summary>
        <div className="mt-2 overflow-x-auto">{table}</div>
      </details>
    </figure>
  );
}

const th = "py-1.5 pr-3 text-left text-xs font-normal text-muted";
const td = "py-1.5 pr-3";
const tdNum = "py-1.5 pr-3 text-right font-mono";

function Overview({ data, state, today, onOpen }: { data: BudgetData; state: BudgetState; today: string; onOpen: (v: View) => void }) {
  const [preset, setPreset] = useState<RangePreset>("this-month");
  const range = presetRange(preset, today);
  const t = totals(data.transactions, range);
  const money = (v: number) => formatMoney(v, data.currency);
  const months = monthlyTotals(data.transactions, monthOf(today), 6);
  const cats = byCategory(data, range);
  const lines = budgetLines(data, monthOf(today));
  const due = upcoming(data.templates, today, 14);
  const peak = Math.max(1, ...months.flatMap((m) => [m.income, m.spending]));
  const monthName = (m: string) => new Date(`${m}-15T12:00:00`).toLocaleDateString(undefined, { month: "short" });
  const catMax = Math.max(1, ...cats.map((c) => c.amount));

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <div className="flex flex-col gap-5">
        <Panel title="summary" actions={<RangePicker value={preset} onChange={setPreset} />}>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <Stat label="Income" value={money(t.income)} />
            <Stat label="Spending" value={money(t.spending)} />
            <Stat label="Net" value={formatMoney(t.net, data.currency, { sign: true })} tone={t.net < 0 ? "bad" : undefined} />
            <Stat label="All accounts" value={money(netWorth(data))} />
          </div>
          <p className="mt-2 text-xs text-muted">Transfers between your accounts aren't counted as income or spending.</p>
          {t.count === 0 && <p className="mt-3 text-sm text-muted">No income or spending recorded in this period.</p>}
        </Panel>

        <Panel title="last six months">
          {months.every((m) => m.income === 0 && m.spending === 0) ? (
            <p className="text-sm text-muted">Record or import transactions to see months side by side.</p>
          ) : (
            <Figure
              label={`Income and spending by month, ${months[0].month} to ${months[5].month}`}
              chart={
                <div className="flex flex-col gap-2">
                  <svg viewBox="0 0 600 180" className="h-44 w-full" preserveAspectRatio="none" aria-hidden>
                    {[0.25, 0.5, 0.75, 1].map((f) => (
                      <line key={f} x1="0" x2="600" y1={160 - f * 150} y2={160 - f * 150} stroke="var(--line)" strokeDasharray="2 4" />
                    ))}
                    {months.map((m, i) => {
                      const x = i * 100 + 22;
                      const hi = (m.income / peak) * 150;
                      const hs = (m.spending / peak) * 150;
                      return (
                        <g key={m.month}>
                          <rect x={x} y={160 - hi} width="26" height={hi} rx="3" fill="var(--periwinkle)" />
                          <rect x={x + 30} y={160 - hs} width="26" height={hs} rx="3" fill="var(--accent)" />
                        </g>
                      );
                    })}
                    <line x1="0" x2="600" y1="160" y2="160" stroke="var(--line)" />
                  </svg>
                  <div className="grid grid-cols-6 text-center font-mono text-[11px] text-muted">
                    {months.map((m) => (
                      <span key={m.month}>{monthName(m.month)}</span>
                    ))}
                  </div>
                  <div className="flex gap-4 text-xs text-muted">
                    <span className="flex items-center gap-1.5">
                      <span className="size-2.5 rounded-[3px] bg-periwinkle" aria-hidden /> Income
                    </span>
                    <span className="flex items-center gap-1.5">
                      <span className="size-2.5 rounded-[3px] bg-accent" aria-hidden /> Spending
                    </span>
                  </div>
                </div>
              }
              table={
                <table className="w-full text-sm">
                  <thead>
                    <tr>
                      <th className={th}>Month</th>
                      <th className={`${th} text-right`}>Income</th>
                      <th className={`${th} text-right`}>Spending</th>
                      <th className={`${th} text-right`}>Net</th>
                    </tr>
                  </thead>
                  <tbody>
                    {months.map((m) => (
                      <tr key={m.month} className="border-t border-line">
                        <td className={td}>{m.month}</td>
                        <td className={tdNum}>{money(m.income)}</td>
                        <td className={tdNum}>{money(m.spending)}</td>
                        <td className={tdNum}>{formatMoney(m.income - m.spending, data.currency, { sign: true })}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              }
            />
          )}
        </Panel>

        <Panel title={`spending by category · ${RANGE_PRESETS.find((p) => p.id === preset)?.label.toLowerCase()}`}>
          {cats.length === 0 ? (
            <p className="text-sm text-muted">No spending in this period.</p>
          ) : (
            <Figure
              label="Spending by category, largest first"
              chart={
                <ul className="flex flex-col gap-2">
                  {cats.map((c) => (
                    <li key={c.id ?? "none"} className="grid grid-cols-[minmax(6rem,9rem)_1fr_auto] items-center gap-3 text-sm">
                      <span className="truncate">{c.name}</span>
                      <span className="h-2 overflow-hidden rounded-full bg-sunken">
                        <span className="block h-full rounded-full" style={{ width: `${(c.amount / catMax) * 100}%`, background: c.color }} />
                      </span>
                      <span className="font-mono text-xs">{money(c.amount)}</span>
                    </li>
                  ))}
                </ul>
              }
              table={
                <table className="w-full text-sm">
                  <thead>
                    <tr>
                      <th className={th}>Category</th>
                      <th className={`${th} text-right`}>Spent</th>
                      <th className={`${th} text-right`}>Share</th>
                    </tr>
                  </thead>
                  <tbody>
                    {cats.map((c) => (
                      <tr key={c.id ?? "none"} className="border-t border-line">
                        <td className={td}>{c.name}</td>
                        <td className={tdNum}>{money(c.amount)}</td>
                        <td className={tdNum}>{Math.round((c.amount / Math.max(1, t.spending)) * 100)}%</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              }
            />
          )}
        </Panel>
      </div>

      <div className="flex flex-col gap-5">
        <Panel title={`this month's budgets · ${new Date(`${today}T12:00:00`).toLocaleDateString(undefined, { month: "long" })}`}>
          {lines.length === 0 ? (
            <Empty title="No monthly budgets yet" action={<Button className={small} onClick={() => onOpen("budgets")}>Set a budget</Button>}>
              Give a spending category a monthly amount to see what's left.
            </Empty>
          ) : (
            <ul className="flex flex-col gap-3">
              {lines.map((l) => (
                <BudgetMeter key={l.category.id} name={l.category.name} color={l.category.color} budget={l.budget} spent={l.spent} currency={data.currency} />
              ))}
            </ul>
          )}
        </Panel>

        <Panel title={`recurring · next two weeks`}>
          {due.length === 0 ? (
            <p className="text-sm text-muted">Nothing recurring in the next two weeks.</p>
          ) : (
            <ul className="flex flex-col gap-2">
              {due.slice(0, 8).map((o) => (
                <li key={`${o.template.id}:${o.date}`} className="flex items-center gap-3 text-sm">
                  <span className={`w-14 shrink-0 font-mono text-xs ${o.due ? "text-warn" : "text-muted"}`}>{o.date.slice(5)}</span>
                  <span className="min-w-0 flex-1 truncate">{o.template.payee || FLOW_LABEL[o.template.kind]}</span>
                  <span className="font-mono text-xs">{money(o.template.amount)}</span>
                  {o.due && o.postable ? (
                    <Button className={small} onClick={() => void state.post(o.template.id)} aria-label={`Record ${o.template.payee || "item"} for ${o.date}`}>
                      Record
                    </Button>
                  ) : (
                    <span className="w-[4.6rem] text-right text-xs text-muted">{o.due ? "after" : "pending"}</span>
                  )}
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </div>
    </div>
  );
}

function BudgetMeter({ name, color, budget, spent, currency }: { name: string; color: string; budget: number; spent: number; currency: string }) {
  const pct = budget > 0 ? Math.min(100, Math.round((spent / budget) * 100)) : 100;
  const over = spent > budget;
  return (
    <li className="flex flex-col gap-1.5">
      <div className="flex items-baseline gap-2 text-sm">
        <span className="size-2 shrink-0 rounded-full" style={{ background: color }} aria-hidden />
        <span className="min-w-0 flex-1 truncate">{name}</span>
        <span className={`font-mono text-xs ${over ? "text-bad" : "text-muted"}`}>
          {over ? `${formatMoney(spent - budget, currency)} over` : `${formatMoney(budget - spent, currency)} left`}
        </span>
      </div>
      <div role="meter" aria-label={`${name}: ${formatMoney(spent, currency)} of ${formatMoney(budget, currency)}`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct} className="h-1.5 overflow-hidden rounded-full bg-sunken">
        <div className={`h-full rounded-full ${over ? "bg-bad" : "bg-accent"}`} style={{ width: `${pct}%` }} />
      </div>
    </li>
  );
}

// ---- Transactions ----

function Transactions({ data, state, today }: { data: BudgetData; state: BudgetState; today: string }) {
  const [preset, setPreset] = useState<RangePreset>("this-month");
  const [account, setAccount] = useState("");
  const [kind, setKind] = useState<"" | Flow>("");
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<string>();
  const range = presetRange(preset, today);
  const name = (id: string | null, list: { id: string; name: string }[]) => list.find((x) => x.id === id)?.name ?? "";
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return sortTransactions(data.transactions).filter(
      (t) =>
        (!range.from || t.date >= range.from) &&
        (!range.to || t.date <= range.to) &&
        (!account || t.account_id === account || t.to_account_id === account) &&
        (!kind || t.kind === kind) &&
        (!q || t.payee.toLowerCase().includes(q) || t.note.toLowerCase().includes(q)),
    );
  }, [data.transactions, range.from, range.to, account, kind, query]);
  const t = totals(shown, {});

  return (
    <Panel title={`transactions · ${shown.length}`} actions={<RangePicker value={preset} onChange={setPreset} />}>
      <div className="mb-3 flex flex-wrap gap-2">
        <input className={`${inputClass} min-h-9 flex-1 text-sm`} placeholder="Search payee or note" aria-label="Search payee or note" value={query} onChange={(e) => setQuery(e.target.value)} />
        <select className={`${inputClass} min-h-9 text-sm`} aria-label="Account" value={account} onChange={(e) => setAccount(e.target.value)}>
          <option value="">All accounts</option>
          {data.accounts.map((a) => (
            <option key={a.id} value={a.id}>
              {a.name}
            </option>
          ))}
        </select>
        <select className={`${inputClass} min-h-9 text-sm`} aria-label="Type" value={kind} onChange={(e) => setKind(e.target.value as "" | Flow)}>
          <option value="">All types</option>
          <option value="expense">Spending</option>
          <option value="income">Income</option>
          <option value="transfer">Transfers</option>
        </select>
      </div>
      <p className="mb-2 font-mono text-xs text-muted">
        In {formatMoney(t.income, data.currency)} · out {formatMoney(t.spending, data.currency)}
      </p>
      {shown.length === 0 ? (
        <p className="text-sm text-muted">{data.transactions.length === 0 ? "No transactions yet. Add one, or import a CSV file." : "Nothing matches these filters."}</p>
      ) : (
        <ul className="-mx-2 flex flex-col">
          {shown.slice(0, 300).map((x) =>
            editing === x.id ? (
              <li key={x.id} className="rounded-sm bg-sunken/60 p-3">
                <TransactionForm
                  data={data}
                  initial={x}
                  kind={x.kind}
                  today={today}
                  onCancel={() => setEditing(undefined)}
                  onDelete={async () => {
                    if (await state.remove("transactions", x.id)) setEditing(undefined);
                  }}
                  onSave={async (input) => {
                    const ok = (await state.save("transactions", txnInput(input), x.id)) !== undefined;
                    if (ok) setEditing(undefined);
                    return ok;
                  }}
                />
              </li>
            ) : (
              <li key={x.id}>
                <button type="button" onClick={() => setEditing(x.id)} className="flex w-full items-center gap-3 rounded-sm px-2 py-2 text-left text-sm hover:bg-sunken/60">
                  <span className="w-12 shrink-0 font-mono text-xs text-muted">{x.date.slice(5)}</span>
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate">{x.payee || FLOW_LABEL[x.kind]}</span>
                    <span className="truncate text-xs text-muted">
                      {x.kind === "transfer" ? `${name(x.account_id, data.accounts)} → ${name(x.to_account_id, data.accounts)}` : [name(x.category_id, data.categories) || "Uncategorized", name(x.account_id, data.accounts)].join(" · ")}
                      {x.template_id ? " · recurring" : ""}
                    </span>
                  </span>
                  <span className={`font-mono text-[13px] ${x.kind === "income" ? "text-ok" : x.kind === "transfer" ? "text-muted" : ""}`}>
                    {x.kind === "income" ? "+" : x.kind === "expense" ? "−" : ""}
                    {formatMoney(x.amount, data.currency)}
                  </span>
                </button>
              </li>
            ),
          )}
        </ul>
      )}
      {shown.length > 300 && <p className="mt-2 text-xs text-muted">Showing the newest 300. Narrow the period or search to find older ones.</p>}
      {data.transactions.length >= MAX_TRANSACTIONS * 0.9 && (
        <p className="mt-2 text-xs text-warn">
          {data.transactions.length} of {MAX_TRANSACTIONS} transactions stored.
        </p>
      )}
    </Panel>
  );
}

const txnInput = (i: TransactionInput): TransactionInput => ({ kind: i.kind, account_id: i.account_id, to_account_id: i.to_account_id, category_id: i.category_id, amount: i.amount, date: i.date, payee: i.payee, note: i.note });

function amountText(minor: number | undefined, currency: string) {
  return minor === undefined ? "" : toDecimal(minor, currency);
}

function TransactionForm(props: {
  data: BudgetData;
  kind: Flow;
  today: string;
  initial?: Transaction | Template;
  recurring?: boolean;
  onSave: (input: TransactionInput & TemplateInput) => Promise<boolean>;
  onCancel?: () => void;
  onDelete?: () => void;
}) {
  const { data, initial, recurring } = props;
  const active = data.accounts.filter((a) => !a.archived || a.id === initial?.account_id || a.id === initial?.to_account_id);
  const [kind, setKind] = useState<Flow>(initial?.kind ?? props.kind);
  const [amount, setAmount] = useState(amountText(initial?.amount, data.currency));
  const [account, setAccount] = useState(initial?.account_id ?? active[0]?.id ?? "");
  const [to, setTo] = useState(initial?.to_account_id ?? active.find((a) => a.id !== account)?.id ?? "");
  const [category, setCategory] = useState(initial?.category_id ?? "");
  const [date, setDate] = useState((initial && ("date" in initial ? initial.date : initial.next_date)) || props.today);
  const [payee, setPayee] = useState(initial?.payee ?? "");
  const [note, setNote] = useState(initial && "note" in initial ? initial.note : "");
  const [cadence, setCadence] = useState<Cadence>(initial && "cadence" in initial ? initial.cadence : "monthly");
  const [problem, setProblem] = useState<string>();
  const [busy, setBusy] = useState(false);
  const first = useRef<HTMLInputElement>(null);
  useEffect(() => first.current?.focus(), []);

  const cats = data.categories.filter((c) => c.kind === kind);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const minor = parseMoney(amount, data.currency);
    const base = {
      kind,
      account_id: account,
      to_account_id: kind === "transfer" ? to : null,
      category_id: kind === "transfer" ? null : category || null,
      amount: minor === undefined ? 0 : Math.abs(minor),
      payee: payee.trim(),
    };
    const txn: TransactionInput = { ...base, date, note: note.trim() };
    const tpl: TemplateInput = { ...base, next_date: date, cadence, active: initial && "active" in initial ? initial.active : true };
    const p = checkTransaction(data, recurring ? tpl : txn);
    if (minor === undefined) return setProblem("Enter the amount as a number, like 12.50.");
    if (p) return setProblem(p);
    setProblem(undefined);
    setBusy(true);
    await props.onSave({ ...txn, ...tpl });
    setBusy(false);
  };

  const field = "flex flex-col gap-1 text-xs text-muted";
  return (
    <form onSubmit={submit} className="flex flex-col gap-3" aria-label={recurring ? "Recurring item" : "Transaction"}>
      <div role="radiogroup" aria-label="Type" className="flex gap-1">
        {(["expense", "income", "transfer"] as Flow[]).map((k) => (
          <label key={k} className={`press flex min-h-9 cursor-pointer items-center rounded-full border px-3 text-[13px] has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-accent ${kind === k ? "border-ink bg-ink text-canvas" : "border-line text-muted"}`}>
            <input type="radio" name="kind" value={k} checked={kind === k} onChange={() => (setKind(k), setCategory(""))} className="sr-only" />
            {FLOW_LABEL[k]}
          </label>
        ))}
      </div>
      <div className="grid gap-3 sm:grid-cols-2">
        <label className={field}>
          Amount ({data.currency})
          <input ref={first} className={`${inputClass} font-mono`} inputMode="decimal" required value={amount} onChange={(e) => setAmount(e.target.value)} placeholder="0.00" />
        </label>
        <label className={field}>
          {recurring ? "Next date" : "Date"}
          <input className={inputClass} type="date" required value={date} onChange={(e) => setDate(e.target.value)} />
        </label>
        <label className={field}>
          {kind === "transfer" ? "From account" : "Account"}
          <select className={inputClass} value={account} onChange={(e) => setAccount(e.target.value)}>
            {active.map((a) => (
              <option key={a.id} value={a.id}>
                {a.name}
              </option>
            ))}
          </select>
        </label>
        {kind === "transfer" ? (
          <label className={field}>
            To account
            <select className={inputClass} value={to} onChange={(e) => setTo(e.target.value)}>
              <option value="">Choose…</option>
              {active
                .filter((a) => a.id !== account)
                .map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
            </select>
          </label>
        ) : (
          <label className={field}>
            Category
            <select className={inputClass} value={category} onChange={(e) => setCategory(e.target.value)}>
              <option value="">Uncategorized</option>
              {cats.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
        )}
        <label className={field}>
          {kind === "income" ? "From" : kind === "transfer" ? "Description" : "Payee"}
          <input className={inputClass} maxLength={120} value={payee} onChange={(e) => setPayee(e.target.value)} />
        </label>
        {recurring ? (
          <label className={field}>
            Repeats
            <select className={inputClass} value={cadence} onChange={(e) => setCadence(e.target.value as Cadence)}>
              {CADENCES.map((c) => (
                <option key={c} value={c}>
                  {c === "biweekly" ? "Every two weeks" : c[0].toUpperCase() + c.slice(1)}
                </option>
              ))}
            </select>
          </label>
        ) : (
          <label className={field}>
            Note
            <input className={inputClass} maxLength={1000} value={note} onChange={(e) => setNote(e.target.value)} />
          </label>
        )}
      </div>
      {problem && (
        <p role="alert" className="text-sm text-bad">
          {problem}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary" disabled={busy}>
          {initial ? "Save" : "Add"}
        </Button>
        {props.onCancel && (
          <Button variant="ghost" onClick={props.onCancel}>
            Cancel
          </Button>
        )}
        {props.onDelete && (
          <Button variant="danger" className="ml-auto" onClick={props.onDelete}>
            Delete
          </Button>
        )}
      </div>
    </form>
  );
}

// ---- Accounts ----

function Accounts({ data, state }: { data: BudgetData; state: BudgetState }) {
  const [editing, setEditing] = useState<string | "new">();
  const bal = balances(data);
  const shown = [...data.accounts].sort((a, b) => Number(a.archived) - Number(b.archived) || a.name.localeCompare(b.name));
  return (
    <Panel title={`accounts · ${formatMoney(netWorth(data), data.currency)} in all`} actions={<Button className={small} onClick={() => setEditing("new")}>Add account</Button>}>
      {editing === "new" && (
        <div className="mb-3 rounded-sm bg-sunken/60 p-3">
          <AccountForm data={data} onCancel={() => setEditing(undefined)} onSave={async (input) => (await state.save("accounts", input)) !== undefined && (setEditing(undefined), true)} />
        </div>
      )}
      <ul className="-mx-2 flex flex-col">
        {shown.map((a) =>
          editing === a.id ? (
            <li key={a.id} className="rounded-sm bg-sunken/60 p-3">
              <AccountForm
                data={data}
                initial={a}
                onCancel={() => setEditing(undefined)}
                onDelete={async () => {
                  if (await state.remove("accounts", a.id)) setEditing(undefined);
                }}
                onSave={async (input) => (await state.save("accounts", input, a.id)) !== undefined && (setEditing(undefined), true)}
              />
            </li>
          ) : (
            <li key={a.id}>
              <button type="button" onClick={() => setEditing(a.id)} className="flex w-full items-center gap-3 rounded-sm px-2 py-2.5 text-left text-sm hover:bg-sunken/60">
                <span className={`min-w-0 flex-1 truncate ${a.archived ? "text-muted" : ""}`}>{a.name}</span>
                <span className="text-xs text-muted">{a.archived ? "archived" : a.kind}</span>
                <span className={`w-28 text-right font-mono text-[13px] ${(bal.get(a.id) ?? 0) < 0 ? "text-bad" : ""}`}>{formatMoney(bal.get(a.id) ?? 0, data.currency)}</span>
              </button>
            </li>
          ),
        )}
      </ul>
      <p className="mt-3 text-xs text-muted">Balance = opening balance + income and transfers in − spending and transfers out. An account with transactions can be archived, not deleted.</p>
    </Panel>
  );
}

function AccountForm({ data, initial, onSave, onCancel, onDelete }: { data: BudgetData; initial?: Account; onSave: (a: { name: string; kind: AccountKind; opening_balance: number; archived: boolean }) => Promise<boolean>; onCancel?: () => void; onDelete?: () => void }) {
  const [name, setName] = useState(initial?.name ?? "");
  const [kind, setKind] = useState<AccountKind>(initial?.kind ?? "chequing");
  const [opening, setOpening] = useState(initial ? toDecimal(initial.opening_balance, data.currency) : "");
  const [archived, setArchived] = useState(initial?.archived ?? false);
  const [problem, setProblem] = useState<string>();
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const minor = opening.trim() === "" ? 0 : parseMoney(opening, data.currency);
    if (!name.trim()) return setProblem("Give the account a name.");
    if (minor === undefined) return setProblem("Enter the opening balance as a number, like 1200.00 (negative for money owed).");
    setProblem(undefined);
    await onSave({ name: name.trim(), kind, opening_balance: minor, archived });
  };
  const field = "flex flex-col gap-1 text-xs text-muted";
  return (
    <form onSubmit={submit} className="flex flex-col gap-3" aria-label="Account">
      <div className="grid gap-3 sm:grid-cols-3">
        <label className={field}>
          Name
          <input className={inputClass} maxLength={60} required value={name} onChange={(e) => setName(e.target.value)} placeholder="Chequing" />
        </label>
        <label className={field}>
          Kind
          <select className={inputClass} value={kind} onChange={(e) => setKind(e.target.value as AccountKind)}>
            {ACCOUNT_KINDS.map((k) => (
              <option key={k} value={k}>
                {k[0].toUpperCase() + k.slice(1)}
              </option>
            ))}
          </select>
        </label>
        <label className={field}>
          Opening balance ({data.currency})
          <input className={`${inputClass} font-mono`} inputMode="decimal" value={opening} onChange={(e) => setOpening(e.target.value)} placeholder="0.00" />
        </label>
      </div>
      {initial && (
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={archived} onChange={(e) => setArchived(e.target.checked)} /> Archived (hidden from new transactions and the total)
        </label>
      )}
      {problem && (
        <p role="alert" className="text-sm text-bad">
          {problem}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary">
          {initial ? "Save" : "Add account"}
        </Button>
        {onCancel && (
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        )}
        {onDelete && (
          <Button variant="danger" className="ml-auto" onClick={onDelete}>
            Delete
          </Button>
        )}
      </div>
    </form>
  );
}

// ---- Categories and monthly budgets ----

function Categories({ data, state, today }: { data: BudgetData; state: BudgetState; today: string }) {
  const [editing, setEditing] = useState<string | "new-expense" | "new-income">();
  const lines = new Map(budgetLines(data, monthOf(today)).map((l) => [l.category.id, l]));
  const group = (kind: "expense" | "income") => {
    const list = data.categories.filter((c) => c.kind === kind);
    const isNew = editing === `new-${kind}`;
    return (
      <Panel
        title={kind === "expense" ? "spending categories" : "income categories"}
        actions={
          <Button className={small} onClick={() => setEditing(`new-${kind}`)}>
            Add
          </Button>
        }
      >
        {isNew && (
          <div className="mb-3 rounded-sm bg-sunken/60 p-3">
            <CategoryForm data={data} kind={kind} onCancel={() => setEditing(undefined)} onSave={async (i) => (await state.save("categories", i)) !== undefined && (setEditing(undefined), true)} />
          </div>
        )}
        {list.length === 0 && !isNew ? (
          <p className="text-sm text-muted">{kind === "expense" ? "No spending categories yet. Add groceries, rent, or whatever you want to track." : "No income categories yet."}</p>
        ) : (
          <ul className="-mx-2 flex flex-col">
            {list.map((c) =>
              editing === c.id ? (
                <li key={c.id} className="rounded-sm bg-sunken/60 p-3">
                  <CategoryForm
                    data={data}
                    kind={kind}
                    initial={c}
                    onCancel={() => setEditing(undefined)}
                    onDelete={async () => {
                      if (await state.remove("categories", c.id)) setEditing(undefined);
                    }}
                    onSave={async (i) => (await state.save("categories", i, c.id)) !== undefined && (setEditing(undefined), true)}
                  />
                </li>
              ) : (
                <li key={c.id}>
                  <button type="button" onClick={() => setEditing(c.id)} className="flex w-full items-center gap-3 rounded-sm px-2 py-2.5 text-left text-sm hover:bg-sunken/60">
                    <span className="size-2.5 shrink-0 rounded-full" style={{ background: c.color }} aria-hidden />
                    <span className="min-w-0 flex-1 truncate">{c.name}</span>
                    {c.monthly_budget !== null && (
                      <span className="font-mono text-xs text-muted">
                        {formatMoney(lines.get(c.id)?.spent ?? 0, data.currency)} of {formatMoney(c.monthly_budget, data.currency)}
                      </span>
                    )}
                  </button>
                </li>
              ),
            )}
          </ul>
        )}
      </Panel>
    );
  };
  return (
    <div className="grid items-start gap-5 lg:grid-cols-2">
      {group("expense")}
      {group("income")}
    </div>
  );
}

function CategoryForm({ data, kind, initial, onSave, onCancel, onDelete }: { data: BudgetData; kind: "expense" | "income"; initial?: BudgetCategory; onSave: (c: { name: string; kind: Flow; color: string; monthly_budget: number | null }) => Promise<boolean>; onCancel: () => void; onDelete?: () => void }) {
  const [name, setName] = useState(initial?.name ?? "");
  const [color, setColor] = useState(initial?.color ?? PALETTE[0].color);
  const [budget, setBudget] = useState(initial?.monthly_budget != null ? toDecimal(initial.monthly_budget, data.currency) : "");
  const [problem, setProblem] = useState<string>();
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const minor = budget.trim() === "" ? null : parseMoney(budget, data.currency);
    if (!name.trim()) return setProblem("Give the category a name.");
    if (minor === undefined || (minor !== null && minor < 0)) return setProblem("Enter the monthly budget as a number, or leave it empty.");
    setProblem(undefined);
    await onSave({ name: name.trim(), kind, color, monthly_budget: kind === "expense" ? minor : null });
  };
  const field = "flex flex-col gap-1 text-xs text-muted";
  return (
    <form onSubmit={submit} className="flex flex-col gap-3" aria-label="Category">
      <div className="grid gap-3 sm:grid-cols-2">
        <label className={field}>
          Name
          <input className={inputClass} maxLength={60} required value={name} onChange={(e) => setName(e.target.value)} autoFocus />
        </label>
        {kind === "expense" && (
          <label className={field}>
            Monthly budget ({data.currency}, optional)
            <input className={`${inputClass} font-mono`} inputMode="decimal" value={budget} onChange={(e) => setBudget(e.target.value)} placeholder="None" />
          </label>
        )}
      </div>
      <fieldset className="flex flex-wrap items-center gap-1.5">
        <legend className="mb-1 text-xs text-muted">Colour</legend>
        {PALETTE.map((p) => (
          <label key={p.color} className="press cursor-pointer rounded-full p-0.5 has-[:checked]:ring-2 has-[:checked]:ring-ink has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-accent" title={p.label}>
            <input type="radio" name="color" className="sr-only" checked={color === p.color} onChange={() => setColor(p.color)} aria-label={p.label} />
            <span className="block size-5 rounded-full" style={{ background: p.color }} />
          </label>
        ))}
      </fieldset>
      {problem && (
        <p role="alert" className="text-sm text-bad">
          {problem}
        </p>
      )}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" variant="primary">
          {initial ? "Save" : "Add category"}
        </Button>
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        {onDelete && (
          <Button variant="danger" className="ml-auto" onClick={onDelete}>
            Delete
          </Button>
        )}
      </div>
      {onDelete && <p className="text-xs text-muted">Deleting keeps its transactions; they become uncategorized.</p>}
    </form>
  );
}

// ---- Recurring ----

function Recurring({ data, state, today }: { data: BudgetData; state: BudgetState; today: string }) {
  const [editing, setEditing] = useState<string | "new">();
  const money = (v: number) => formatMoney(v, data.currency);
  const sorted = [...data.templates].sort((a, b) => Number(b.active) - Number(a.active) || a.next_date.localeCompare(b.next_date));
  const save = (id?: string) => async (i: TemplateInput) =>
    (await state.save("templates", { kind: i.kind, account_id: i.account_id, to_account_id: i.to_account_id, category_id: i.category_id, amount: i.amount, payee: i.payee, cadence: i.cadence, next_date: i.next_date, active: i.active }, id)) !== undefined && (setEditing(undefined), true);
  return (
    <Panel title={`recurring · ${data.templates.filter((t) => t.active).length} active`} actions={<Button className={small} onClick={() => setEditing("new")}>Add recurring</Button>}>
      <p className="mb-3 max-w-prose text-sm text-muted">Rent, pay, and subscriptions. Each date stays pending until you record it, so the totals only include money that actually moved.</p>
      {editing === "new" && (
        <div className="mb-3 rounded-sm bg-sunken/60 p-3">
          <TransactionForm data={data} kind="expense" today={today} recurring onCancel={() => setEditing(undefined)} onSave={save()} />
        </div>
      )}
      {sorted.length === 0 && editing !== "new" ? (
        <p className="text-sm text-muted">No recurring items yet.</p>
      ) : (
        <ul className="-mx-2 flex flex-col">
          {sorted.map((t) =>
            editing === t.id ? (
              <li key={t.id} className="rounded-sm bg-sunken/60 p-3">
                <TransactionForm
                  data={data}
                  kind={t.kind}
                  initial={t}
                  today={today}
                  recurring
                  onCancel={() => setEditing(undefined)}
                  onDelete={async () => {
                    if (await state.remove("templates", t.id)) setEditing(undefined);
                  }}
                  onSave={save(t.id)}
                />
              </li>
            ) : (
              <li key={t.id} className="flex items-center gap-2 px-2 py-2 text-sm">
                <button type="button" onClick={() => setEditing(t.id)} className="flex min-w-0 flex-1 flex-col text-left hover:underline">
                  <span className={`truncate ${t.active ? "" : "text-muted"}`}>{t.payee || FLOW_LABEL[t.kind]}</span>
                  <span className="truncate text-xs text-muted">
                    {t.cadence === "biweekly" ? "Every two weeks" : t.cadence[0].toUpperCase() + t.cadence.slice(1)} · {t.active ? `next ${t.next_date}` : "paused"}
                  </span>
                </button>
                <span className="font-mono text-xs">{money(t.amount)}</span>
                <Button className={small} variant="ghost" onClick={() => void save(t.id)({ ...t, active: !t.active })}>
                  {t.active ? "Pause" : "Resume"}
                </Button>
                <Button className={small} disabled={!t.active || t.next_date > today} onClick={() => void state.post(t.id)} title={t.next_date > today ? `Due ${t.next_date}` : undefined}>
                  Record
                </Button>
              </li>
            ),
          )}
        </ul>
      )}
    </Panel>
  );
}

// ---- Import and export ----

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/csv;charset=utf-8" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function Files({ data, state, today }: { data: BudgetData; state: BudgetState; today: string }) {
  const [preset, setPreset] = useState<RangePreset>("all");
  const [text, setText] = useState<string>();
  const [fileName, setFileName] = useState("");
  const [account, setAccount] = useState(data.accounts.find((a) => !a.archived)?.id ?? "");
  const [withDupes, setWithDupes] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string>();
  const [currency, setCurrency] = useState(data.currency);
  const preview = useMemo(() => (text === undefined ? undefined : previewImport(text, data, account, clientId)), [text, data, account]);
  const rows = preview?.rows ?? [];
  const ready = rows.filter((r): r is ImportRow & { input: NonNullable<ImportRow["input"]> } => !!r.input && (withDupes || !r.duplicate));
  const errors = rows.filter((r) => r.error).length;
  const dupes = rows.filter((r) => r.duplicate).length;

  const run = async () => {
    setBusy(true);
    const r = await state.importRows(ready.map((x) => x.input));
    setBusy(false);
    if (r) {
      setResult(`${r.imported} imported${r.skipped ? `, ${r.skipped} already there` : ""}.`);
      setText(undefined);
      setFileName("");
    }
  };

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <Panel title="import a CSV file">
        <p className="mb-3 max-w-prose text-sm text-muted">
          Use a CSV export from your bank or a spreadsheet. The first row needs column names, including <span className="font-mono">date</span> (YYYY-MM-DD) and <span className="font-mono">amount</span>, or <span className="font-mono">debit</span> and <span className="font-mono">credit</span>. Negative amounts count as spending. You'll see every row before anything is saved.
        </p>
        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1 text-xs text-muted">
            File
            <input
              type="file"
              accept=".csv,text/csv"
              className="text-sm text-ink file:mr-3 file:rounded-full file:border file:border-line file:bg-panel file:px-3 file:py-1.5 file:text-[13px] file:text-ink"
              onChange={async (e) => {
                const f = e.target.files?.[0];
                setResult(undefined);
                if (!f) return;
                if (f.size > 2_000_000) return setResult("That file is over 2 MB. Split it into smaller files.");
                setFileName(f.name);
                setText(await f.text());
              }}
            />
          </label>
          <label className="flex flex-col gap-1 text-xs text-muted">
            Account for rows without one
            <select className={`${inputClass} min-h-9 text-sm`} value={account} onChange={(e) => setAccount(e.target.value)}>
              {data.accounts
                .filter((a) => !a.archived)
                .map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.name}
                  </option>
                ))}
            </select>
          </label>
        </div>
        {result && (
          <p role="status" className="mt-3 text-sm">
            {result}
          </p>
        )}
        {preview?.error && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {preview.error}
          </p>
        )}
        {preview && !preview.error && (
          <div className="mt-4 flex flex-col gap-3">
            <div className="flex flex-wrap gap-2">
              <StatusChip tone="ok" label="new" value={String(rows.filter((r) => r.input && !r.duplicate).length)} />
              <StatusChip tone={dupes ? "warn" : "idle"} label="look like duplicates" value={String(dupes)} />
              <StatusChip tone={errors ? "bad" : "idle"} label="can't import" value={String(errors)} />
            </div>
            {dupes > 0 && (
              <label className="flex items-center gap-2 text-sm">
                <input type="checkbox" checked={withDupes} onChange={(e) => setWithDupes(e.target.checked)} />
                Import rows that look like duplicates too (same date, amount, payee, and account)
              </label>
            )}
            <div className="max-h-96 overflow-auto rounded-sm border border-line">
              <table className="w-full text-sm">
                <caption className="sr-only">Preview of {fileName}</caption>
                <thead className="sticky top-0 bg-panel">
                  <tr>
                    <th className={`${th} pl-2`}>Line</th>
                    <th className={th}>Date</th>
                    <th className={th}>Payee</th>
                    <th className={`${th} text-right`}>Amount</th>
                    <th className={th}>Status</th>
                  </tr>
                </thead>
                <tbody>
                  {rows.slice(0, 500).map((r) => (
                    <tr key={r.line} className={`border-t border-line ${r.error ? "text-bad" : r.duplicate && !withDupes ? "text-muted" : ""}`}>
                      <td className={`${td} pl-2 font-mono text-xs`}>{r.line}</td>
                      <td className={`${td} font-mono text-xs`}>{r.input?.date ?? ""}</td>
                      <td className={`${td} max-w-48 truncate`}>{r.input?.payee ?? ""}</td>
                      <td className={tdNum}>{r.input ? `${r.input.kind === "expense" ? "−" : r.input.kind === "income" ? "+" : ""}${formatMoney(r.input.amount, data.currency)}` : ""}</td>
                      <td className={`${td} text-xs`}>{r.error ?? (r.duplicate ? (withDupes ? "duplicate, importing" : "duplicate, skipped") : r.unknownCategory ? `new; no category “${r.unknownCategory}”` : "new")}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button variant="primary" disabled={busy || ready.length === 0} onClick={() => void run()}>
                {busy ? "Importing…" : `Import ${ready.length} row${ready.length === 1 ? "" : "s"}`}
              </Button>
              <Button variant="ghost" onClick={() => (setText(undefined), setFileName(""))}>
                Cancel
              </Button>
            </div>
          </div>
        )}
      </Panel>

      <div className="flex flex-col gap-5">
        <Panel title="export">
          <div className="flex flex-col gap-3">
            <RangePicker value={preset} onChange={setPreset} />
            <Button
              disabled={data.transactions.length === 0}
              onClick={() => download(`budget-${today}.csv`, toCsv(data, presetRange(preset, today)))}
            >
              Download CSV
            </Button>
            <p className="text-xs text-muted">One row per transaction, with account and category names. Re-importing the file skips rows already here.</p>
          </div>
        </Panel>
        <Panel title="currency">
          <form
            className="flex flex-wrap items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void state.setCurrency(currency.trim().toUpperCase());
            }}
          >
            <label className="flex flex-col gap-1 text-xs text-muted">
              Code (3 letters)
              <input className={`${inputClass} w-24 font-mono uppercase`} maxLength={3} pattern="[A-Za-z]{3}" required value={currency} onChange={(e) => setCurrency(e.target.value)} />
            </label>
            <Button type="submit" disabled={currency.trim().toUpperCase() === data.currency}>
              Save
            </Button>
          </form>
          <p className="mt-2 text-xs text-muted">One currency for the whole budget. Changing it relabels amounts; it doesn't convert them.</p>
        </Panel>
      </div>
    </div>
  );
}
