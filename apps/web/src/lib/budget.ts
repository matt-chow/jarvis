/**
 * Budget (DECISIONS D-050): accounts, categories, transactions, and
 * recurring templates, stored on the server per owner. Everything shown is
 * derived here from those records, so balances and reports work offline.
 *
 * Amounts are positive integers in minor units (cents for CAD); the kind
 * gives the direction. Transfers move money between accounts and never count
 * as income or spending.
 */

export type Flow = "expense" | "income" | "transfer";
export type Cadence = "weekly" | "biweekly" | "monthly" | "yearly";
export type Entity = "accounts" | "categories" | "transactions" | "templates";

export const ACCOUNT_KINDS = ["cash", "chequing", "savings", "credit", "other"] as const;
export type AccountKind = (typeof ACCOUNT_KINDS)[number];
export const CADENCES: Cadence[] = ["weekly", "biweekly", "monthly", "yearly"];
export const DEFAULT_CURRENCY = "CAD";
export const MAX_TRANSACTIONS = 10_000;
/** Rows per import request (the server caps request bodies at 64 KB). */
export const IMPORT_CHUNK = 100;

export interface Account {
  id: string;
  name: string;
  kind: AccountKind;
  /** Balance before the first recorded transaction (may be negative, e.g. a credit card). */
  opening_balance: number;
  archived: boolean;
  created_at: number;
  updated_at: number;
}

export interface BudgetCategory {
  id: string;
  name: string;
  kind: Flow;
  color: string;
  /** Expense categories only. */
  monthly_budget: number | null;
  created_at: number;
  updated_at: number;
}

export interface Transaction {
  id: string;
  kind: Flow;
  account_id: string;
  to_account_id: string | null;
  category_id: string | null;
  amount: number;
  date: string;
  payee: string;
  note: string;
  /** The recurring template this was posted from. */
  template_id: string | null;
  created_at: number;
  updated_at: number;
}

export interface Template {
  id: string;
  kind: Flow;
  account_id: string;
  to_account_id: string | null;
  category_id: string | null;
  amount: number;
  payee: string;
  cadence: Cadence;
  /** The next occurrence; pending until posted. */
  next_date: string;
  anchor_day: number;
  active: boolean;
  created_at: number;
  updated_at: number;
}

export interface BudgetData {
  currency: string;
  accounts: Account[];
  categories: BudgetCategory[];
  transactions: Transaction[];
  templates: Template[];
}

export interface AccountInput {
  name: string;
  kind: AccountKind;
  opening_balance: number;
  archived: boolean;
}

export interface CategoryInput {
  name: string;
  kind: Flow;
  color: string;
  monthly_budget: number | null;
}

export interface TransactionInput {
  kind: Flow;
  account_id: string;
  to_account_id: string | null;
  category_id: string | null;
  amount: number;
  date: string;
  payee: string;
  note: string;
}

export interface TemplateInput {
  kind: Flow;
  account_id: string;
  to_account_id: string | null;
  category_id: string | null;
  amount: number;
  payee: string;
  cadence: Cadence;
  next_date: string;
  active: boolean;
}

export interface Inputs {
  accounts: AccountInput;
  categories: CategoryInput;
  transactions: TransactionInput;
  templates: TemplateInput;
}

export interface Records {
  accounts: Account;
  categories: BudgetCategory;
  transactions: Transaction;
  templates: Template;
}

/** `POST /api/v1/budget/import`. */
export interface Imported {
  imported: number;
  /** Ids already stored (an earlier attempt, or a re-imported export). */
  skipped: number;
}

export const emptyBudget =(): BudgetData => ({ currency: DEFAULT_CURRENCY, accounts: [], categories: [], transactions: [], templates: [] });

// ---- Money ----

/** Digits after the decimal point for the currency (2 for CAD, 0 for JPY). */
export function minorDigits(currency: string): number {
  try {
    return new Intl.NumberFormat("en", { style: "currency", currency }).resolvedOptions().maximumFractionDigits ?? 2;
  } catch {
    return 2;
  }
}

/** `123456` → `$1,234.56` (in the device's locale). */
export function formatMoney(minor: number, currency: string, opts: { sign?: boolean } = {}): string {
  const digits = minorDigits(currency);
  let f: Intl.NumberFormat;
  try {
    f = new Intl.NumberFormat(undefined, { style: "currency", currency, signDisplay: opts.sign ? "exceptZero" : "auto" });
  } catch {
    f = new Intl.NumberFormat(undefined, { minimumFractionDigits: digits, maximumFractionDigits: digits });
  }
  return f.format(minor / 10 ** digits);
}

/** Minor units as a plain decimal (`-1234.5` → `"-12.35"` style), for inputs and CSV. */
export function toDecimal(minor: number, currency: string): string {
  const digits = minorDigits(currency);
  const sign = minor < 0 ? "-" : "";
  const abs = Math.abs(minor);
  if (digits === 0) return `${sign}${abs}`;
  const unit = 10 ** digits;
  return `${sign}${Math.floor(abs / unit)}.${String(abs % unit).padStart(digits, "0")}`;
}

/**
 * Parse typed or imported money into minor units: `12`, `12.5`, `$1,234.56`,
 * `-4.20`, `(4.20)` (negative). Returns undefined when it isn't a number or
 * has more decimals than the currency allows.
 */
export function parseMoney(text: string, currency: string): number | undefined {
  let s = text.trim().replace(/[\s,]/g, "").replace(/^[A-Za-z]{3}/, "").replace(/[A-Za-z]{3}$/, "");
  let negative = false;
  if (/^\(.*\)$/.test(s)) {
    negative = true;
    s = s.slice(1, -1);
  }
  if (s.startsWith("-")) {
    negative = !negative;
    s = s.slice(1);
  } else if (s.startsWith("+")) s = s.slice(1);
  s = s.replace(/^[^\d.]+/, "");
  const digits = minorDigits(currency);
  const m = /^(\d+)(?:\.(\d*))?$/.exec(s) ?? /^()\.(\d+)$/.exec(s);
  if (!m) return undefined;
  const frac = m[2] ?? "";
  if (frac.length > digits) return undefined;
  const value = Number(m[1] || "0") * 10 ** digits + Number(frac.padEnd(digits, "0") || "0");
  if (!Number.isSafeInteger(value)) return undefined;
  return negative ? -value : value;
}

// ---- Dates ----

const pad = (n: number) => String(n).padStart(2, "0");
const iso = (y: number, m: number, d: number) => `${y}-${pad(m)}-${pad(d)}`;
const daysIn = (y: number, m: number) => new Date(Date.UTC(y, m, 0)).getUTCDate();

export function addDaysIso(date: string, days: number): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + days);
  return d.toISOString().slice(0, 10);
}

export function isIsoDate(s: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(s)) return false;
  const [y, m, d] = s.split("-").map(Number);
  return m >= 1 && m <= 12 && d >= 1 && d <= daysIn(y, m);
}

/** The occurrence after `date` (mirrors core `next_occurrence`). Month ends clamp to the anchor day. */
export function nextOccurrence(date: string, cadence: Cadence, anchorDay: number): string {
  if (cadence === "weekly") return addDaysIso(date, 7);
  if (cadence === "biweekly") return addDaysIso(date, 14);
  const [y, m] = date.split("-").map(Number);
  const [ny, nm] = cadence === "monthly" ? (m === 12 ? [y + 1, 1] : [y, m + 1]) : [y + 1, m];
  return iso(ny, nm, Math.min(anchorDay, daysIn(ny, nm)));
}

export const monthOf = (date: string) => date.slice(0, 7);

export function shiftMonth(month: string, by: number): string {
  const [y, m] = month.split("-").map(Number);
  const i = y * 12 + (m - 1) + by;
  return `${Math.floor(i / 12)}-${pad((i % 12) + 1)}`;
}

export function monthRange(month: string): Range {
  const [y, m] = month.split("-").map(Number);
  return { from: `${month}-01`, to: iso(y, m, daysIn(y, m)) };
}

export interface Range {
  /** Inclusive; undefined = open. */
  from?: string;
  to?: string;
}

export type RangePreset = "this-month" | "last-month" | "3-months" | "this-year" | "all";
export const RANGE_PRESETS: { id: RangePreset; label: string }[] = [
  { id: "this-month", label: "This month" },
  { id: "last-month", label: "Last month" },
  { id: "3-months", label: "Last 3 months" },
  { id: "this-year", label: "This year" },
  { id: "all", label: "All time" },
];

export function presetRange(preset: RangePreset, today: string): Range {
  const month = monthOf(today);
  switch (preset) {
    case "this-month":
      return monthRange(month);
    case "last-month":
      return monthRange(shiftMonth(month, -1));
    case "3-months":
      return { from: monthRange(shiftMonth(month, -2)).from, to: monthRange(month).to };
    case "this-year":
      return { from: `${today.slice(0, 4)}-01-01`, to: `${today.slice(0, 4)}-12-31` };
    case "all":
      return {};
  }
}

export const inRange = (date: string, r: Range) => (!r.from || date >= r.from) && (!r.to || date <= r.to);

// ---- Derived figures ----

/** Each account's balance: opening, plus income and transfers in, minus spending and transfers out. */
export function balances(data: BudgetData, upTo?: string): Map<string, number> {
  const out = new Map(data.accounts.map((a) => [a.id, a.opening_balance]));
  const add = (id: string | null, v: number) => {
    if (id && out.has(id)) out.set(id, out.get(id)! + v);
  };
  for (const t of data.transactions) {
    if (upTo && t.date > upTo) continue;
    if (t.kind === "income") add(t.account_id, t.amount);
    else if (t.kind === "expense") add(t.account_id, -t.amount);
    else {
      add(t.account_id, -t.amount);
      add(t.to_account_id, t.amount);
    }
  }
  return out;
}

/** Sum of balances of accounts that aren't archived. */
export function netWorth(data: BudgetData): number {
  const b = balances(data);
  return data.accounts.filter((a) => !a.archived).reduce((sum, a) => sum + (b.get(a.id) ?? 0), 0);
}

export interface Totals {
  income: number;
  spending: number;
  /** income − spending. */
  net: number;
  count: number;
}

/** Income and spending in the range. Transfers are excluded. */
export function totals(txns: Transaction[], r: Range): Totals {
  let income = 0;
  let spending = 0;
  let count = 0;
  for (const t of txns) {
    if (!inRange(t.date, r) || t.kind === "transfer") continue;
    count++;
    if (t.kind === "income") income += t.amount;
    else spending += t.amount;
  }
  return { income, spending, net: income - spending, count };
}

export interface CategoryTotal {
  /** null = uncategorized. */
  id: string | null;
  name: string;
  color: string;
  amount: number;
}

/** Spending (or income) by category in the range, largest first. */
export function byCategory(data: BudgetData, r: Range, kind: "expense" | "income" = "expense"): CategoryTotal[] {
  const sums = new Map<string | null, number>();
  for (const t of data.transactions) {
    if (t.kind !== kind || !inRange(t.date, r)) continue;
    const id = t.category_id && data.categories.some((c) => c.id === t.category_id) ? t.category_id : null;
    sums.set(id, (sums.get(id) ?? 0) + t.amount);
  }
  return [...sums]
    .map(([id, amount]) => {
      const c = data.categories.find((x) => x.id === id);
      return { id, name: c?.name ?? "Uncategorized", color: c?.color ?? "#8a8c99", amount };
    })
    .sort((a, b) => b.amount - a.amount || a.name.localeCompare(b.name));
}

export interface BudgetLine {
  category: BudgetCategory;
  budget: number;
  spent: number;
  /** budget − spent (negative when over). */
  left: number;
}

/** Expense categories with a monthly budget, against the month's spending. */
export function budgetLines(data: BudgetData, month: string): BudgetLine[] {
  const r = monthRange(month);
  return data.categories
    .filter((c) => c.kind === "expense" && c.monthly_budget !== null)
    .map((c) => {
      const spent = data.transactions.filter((t) => t.kind === "expense" && t.category_id === c.id && inRange(t.date, r)).reduce((s, t) => s + t.amount, 0);
      const budget = c.monthly_budget ?? 0;
      return { category: c, budget, spent, left: budget - spent };
    });
}

export interface MonthTotal {
  month: string;
  income: number;
  spending: number;
}

/** The last `n` months ending with `month`, oldest first. */
export function monthlyTotals(txns: Transaction[], month: string, n: number): MonthTotal[] {
  return Array.from({ length: n }, (_, i) => {
    const m = shiftMonth(month, i - n + 1);
    const t = totals(txns, monthRange(m));
    return { month: m, income: t.income, spending: t.spending };
  });
}

export interface Occurrence {
  template: Template;
  date: string;
  /** On or before today: waiting to be posted. */
  due: boolean;
  /** Only the template's next occurrence can be posted (posting moves it on). */
  postable: boolean;
}

/** Pending occurrences of active templates up to `today + days`, soonest first. */
export function upcoming(templates: Template[], today: string, days = 31): Occurrence[] {
  const until = addDaysIso(today, days);
  const out: Occurrence[] = [];
  for (const t of templates) {
    if (!t.active) continue;
    let date = t.next_date;
    for (let i = 0; i < 60 && date <= until; i++) {
      out.push({ template: t, date, due: date <= today, postable: i === 0 });
      date = nextOccurrence(date, t.cadence, t.anchor_day);
    }
  }
  return out.sort((a, b) => a.date.localeCompare(b.date) || a.template.payee.localeCompare(b.template.payee));
}

// ---- Local application of queued changes (offline) ----

export function recordFrom<E extends Entity>(entity: E, id: string, input: Inputs[E], now: number, old?: Records[E]): Records[E] {
  const created_at = old?.created_at ?? now;
  switch (entity) {
    case "accounts":
      return { id, created_at, updated_at: now, ...(input as AccountInput), name: (input as AccountInput).name.trim() } as Records[E];
    case "categories": {
      const c = input as CategoryInput;
      return { id, created_at, updated_at: now, ...c, name: c.name.trim(), monthly_budget: c.kind === "expense" ? c.monthly_budget : null } as Records[E];
    }
    case "transactions":
      return {
        id,
        created_at,
        updated_at: now,
        template_id: (old as Transaction | undefined)?.template_id ?? null,
        ...(input as TransactionInput),
        payee: (input as TransactionInput).payee.trim(),
        note: (input as TransactionInput).note.trim(),
      } as Records[E];
    default: {
      const t = input as TemplateInput;
      return { id, created_at, updated_at: now, ...t, payee: t.payee.trim(), anchor_day: Number(t.next_date.slice(8, 10)) } as Records[E];
    }
  }
}

/** Replace the record with the same id in its list, or add it. */
export function upsertRecord<E extends Entity>(data: BudgetData, entity: E, record: Records[E]): BudgetData {
  const list = data[entity] as Records[E][];
  const next = list.some((x) => x.id === record.id) ? list.map((x) => (x.id === record.id ? record : x)) : [...list, record];
  return { ...data, [entity]: next };
}

export function removeRecord(data: BudgetData, entity: Entity, id: string): BudgetData {
  const next = { ...data, [entity]: (data[entity] as { id: string }[]).filter((x) => x.id !== id) } as BudgetData;
  if (entity !== "categories") return next;
  const clear = <T extends { category_id: string | null }>(x: T): T => (x.category_id === id ? { ...x, category_id: null } : x);
  return { ...next, transactions: next.transactions.map(clear), templates: next.templates.map(clear) };
}

/** Post a template's next occurrence as transaction `txnId` and move the template on. */
export function postLocally(data: BudgetData, templateId: string, txnId: string, now: number): BudgetData {
  const t = data.templates.find((x) => x.id === templateId);
  if (!t || data.transactions.some((x) => x.id === txnId)) return data;
  const txn: Transaction = {
    id: txnId,
    kind: t.kind,
    account_id: t.account_id,
    to_account_id: t.to_account_id,
    category_id: t.category_id,
    amount: t.amount,
    date: t.next_date,
    payee: t.payee,
    note: "",
    template_id: t.id,
    created_at: now,
    updated_at: now,
  };
  const moved = { ...t, next_date: nextOccurrence(t.next_date, t.cadence, t.anchor_day), updated_at: now };
  return upsertRecord(upsertRecord(data, "transactions", txn), "templates", moved);
}

export function importLocally(data: BudgetData, rows: (TransactionInput & { id: string })[], now: number): BudgetData {
  const known = new Set(data.transactions.map((t) => t.id));
  const added = rows.filter((r) => !known.has(r.id)).map(({ id, ...input }) => recordFrom("transactions", id, input, now));
  return { ...data, transactions: [...data.transactions, ...added] };
}

/** Newest first, then most recently added. */
export const sortTransactions = (txns: Transaction[]) => [...txns].sort((a, b) => b.date.localeCompare(a.date) || b.created_at - a.created_at);

// ---- Checks the server also makes (so offline edits fail early) ----

/** A problem with the input, in words, or undefined when it's fine. */
export function checkTransaction(data: BudgetData, t: TransactionInput | TemplateInput): string | undefined {
  if (!Number.isSafeInteger(t.amount) || t.amount <= 0) return "Enter an amount above zero.";
  if (!data.accounts.some((a) => a.id === t.account_id)) return "Choose an account.";
  if (t.kind === "transfer") {
    if (!t.to_account_id || !data.accounts.some((a) => a.id === t.to_account_id)) return "Choose the account the money goes to.";
    if (t.to_account_id === t.account_id) return "A transfer needs two different accounts.";
  } else if (t.category_id) {
    const c = data.categories.find((x) => x.id === t.category_id);
    if (!c || c.kind !== t.kind) return "Choose a category of the same kind.";
  }
  if ("date" in t && !isIsoDate(t.date)) return "Enter a date.";
  if ("next_date" in t && !isIsoDate(t.next_date)) return "Enter the next date.";
  return undefined;
}

// ---- CSV ----

const CSV_HEADER = ["date", "type", "amount", "account", "to_account", "category", "payee", "note", "id"];

function csvField(v: string): string {
  // Neutralise spreadsheet formulas, and quote when needed.
  const safe = /^[=+\-@\t\r]/.test(v) && !/^-?\d/.test(v) ? `'${v}` : v;
  return /[",\n\r]/.test(safe) ? `"${safe.replace(/"/g, '""')}"` : safe;
}

/** Transactions as CSV (oldest first), amounts as positive decimals. */
export function toCsv(data: BudgetData, r: Range = {}): string {
  const name = (list: { id: string; name: string }[], id: string | null) => (id ? (list.find((x) => x.id === id)?.name ?? "") : "");
  const rows = [...data.transactions]
    .filter((t) => inRange(t.date, r))
    .sort((a, b) => a.date.localeCompare(b.date) || a.created_at - b.created_at)
    .map((t) =>
      [t.date, t.kind, toDecimal(t.amount, data.currency), name(data.accounts, t.account_id), name(data.accounts, t.to_account_id), name(data.categories, t.category_id), t.payee, t.note, t.id].map(csvField).join(","),
    );
  return [CSV_HEADER.join(","), ...rows].join("\r\n") + "\r\n";
}

/** RFC 4180 rows (quoted fields, doubled quotes, CRLF or LF). Blank lines are dropped. */
export function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  const s = text.replace(/^﻿/, "");
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (quoted) {
      if (ch === '"' && s[i + 1] === '"') {
        field += '"';
        i++;
      } else if (ch === '"') quoted = false;
      else field += ch;
    } else if (ch === '"' && field === "") quoted = true;
    else if (ch === ",") {
      row.push(field);
      field = "";
    } else if (ch === "\n" || ch === "\r") {
      if (ch === "\r" && s[i + 1] === "\n") i++;
      row.push(field);
      if (row.some((f) => f.trim() !== "")) rows.push(row);
      row = [];
      field = "";
    } else field += ch;
  }
  row.push(field);
  if (row.some((f) => f.trim() !== "")) rows.push(row);
  return rows;
}

export interface ImportRow {
  /** 1-based line in the file (the header is line 1). */
  line: number;
  input?: TransactionInput & { id: string };
  error?: string;
  /** Matches a recorded transaction, or an earlier row in this file. */
  duplicate: boolean;
  /** A category name in the file that matched nothing (imported uncategorized). */
  unknownCategory?: string;
}

const HEADERS: Record<string, string[]> = {
  date: ["date", "posted", "transaction date", "posting date"],
  amount: ["amount", "value"],
  debit: ["debit", "withdrawal", "withdrawals", "money out"],
  credit: ["credit", "deposit", "deposits", "money in"],
  type: ["type", "kind"],
  payee: ["payee", "description", "merchant", "name", "details"],
  category: ["category"],
  account: ["account"],
  to_account: ["to_account", "to account"],
  note: ["note", "notes", "memo"],
  id: ["id"],
};

/** Which column holds each field, from the header row. */
export function mapHeader(header: string[]): Partial<Record<keyof typeof HEADERS, number>> {
  const out: Partial<Record<keyof typeof HEADERS, number>> = {};
  header.forEach((h, i) => {
    const key = h.trim().toLowerCase();
    for (const [field, names] of Object.entries(HEADERS)) if (out[field as keyof typeof HEADERS] === undefined && names.includes(key)) out[field as keyof typeof HEADERS] = i;
  });
  return out;
}

const dupKey = (t: { date: string; amount: number; payee: string; account_id: string; kind: Flow }) => `${t.date}|${t.kind}|${t.amount}|${t.payee.trim().toLowerCase()}|${t.account_id}`;

/**
 * Preview a CSV import. Needs a header with a date and an amount (or debit
 * and credit) column. Without a type column, negative amounts are spending
 * and positive ones income. Rows with an unknown account go to
 * `defaultAccount`. `newId` makes the client ids that make a retry safe.
 */
export function previewImport(text: string, data: BudgetData, defaultAccount: string, newId: () => string): { rows: ImportRow[]; error?: string } {
  const [header, ...body] = parseCsv(text);
  if (!header) return { rows: [], error: "The file is empty." };
  const col = mapHeader(header);
  if (col.date === undefined || (col.amount === undefined && col.debit === undefined && col.credit === undefined)) {
    return { rows: [], error: "The first row needs column names, including date and amount (or debit and credit)." };
  }
  const byName = <T extends { name: string }>(list: T[], name: string) => list.find((x) => x.name.trim().toLowerCase() === name.trim().toLowerCase());
  const seen = new Set(data.transactions.map(dupKey));
  const known = new Set(data.transactions.map((t) => t.id));
  const cell = (r: string[], i: number | undefined) => (i === undefined ? "" : (r[i] ?? "").trim());
  const rows = body.map((r, n): ImportRow => {
    const line = n + 2;
    const date = cell(r, col.date).replace(/\//g, "-");
    if (!isIsoDate(date)) return { line, duplicate: false, error: "Date must be YYYY-MM-DD." };
    let signed: number | undefined;
    if (col.amount !== undefined && cell(r, col.amount) !== "") signed = parseMoney(cell(r, col.amount), data.currency);
    else {
      const out = cell(r, col.debit) ? parseMoney(cell(r, col.debit), data.currency) : 0;
      const inn = cell(r, col.credit) ? parseMoney(cell(r, col.credit), data.currency) : 0;
      signed = out === undefined || inn === undefined ? undefined : Math.abs(inn) - Math.abs(out);
    }
    if (signed === undefined || signed === 0) return { line, duplicate: false, error: "Amount isn't a number (or is zero)." };
    const typed = cell(r, col.type).toLowerCase();
    const kind: Flow = typed === "income" || typed === "expense" || typed === "transfer" ? typed : signed < 0 ? "expense" : "income";
    const account = byName(data.accounts, cell(r, col.account))?.id ?? defaultAccount;
    const to = kind === "transfer" ? (byName(data.accounts, cell(r, col.to_account))?.id ?? null) : null;
    const catName = cell(r, col.category);
    const cat = catName ? data.categories.find((c) => c.kind === kind && c.name.trim().toLowerCase() === catName.toLowerCase()) : undefined;
    const fileId = cell(r, col.id).toLowerCase();
    const input = {
      id: /^[0-9a-f]{32}$/.test(fileId) ? fileId : newId(),
      kind,
      account_id: account,
      to_account_id: to,
      category_id: cat?.id ?? null,
      amount: Math.abs(signed),
      date,
      payee: cell(r, col.payee).slice(0, 120),
      note: cell(r, col.note).slice(0, 1000),
    };
    const problem = checkTransaction(data, input);
    if (problem) return { line, duplicate: false, error: problem };
    const key = dupKey(input);
    const duplicate = known.has(input.id) || seen.has(key);
    seen.add(key);
    known.add(input.id);
    return { line, input, duplicate, unknownCategory: catName && !cat ? catName : undefined };
  });
  return { rows };
}
