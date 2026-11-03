import { describe, expect, it } from "vitest";
import {
  balances,
  budgetLines,
  byCategory,
  emptyBudget,
  formatMoney,
  importLocally,
  monthlyTotals,
  nextOccurrence,
  parseCsv,
  parseMoney,
  postLocally,
  presetRange,
  previewImport,
  removeRecord,
  toCsv,
  toDecimal,
  totals,
  upcoming,
  type BudgetData,
  type Transaction,
} from "./budget";

const ID = (n: number) => String(n).padStart(32, "0");

function txn(over: Partial<Transaction>): Transaction {
  return {
    id: ID(Math.floor(Math.random() * 1e9)),
    kind: "expense",
    account_id: ID(1),
    to_account_id: null,
    category_id: null,
    amount: 100,
    date: "2026-11-02",
    payee: "",
    note: "",
    template_id: null,
    created_at: 0,
    updated_at: 0,
    ...over,
  };
}

function sample(): BudgetData {
  const b = emptyBudget();
  b.accounts = [
    { id: ID(1), name: "Chequing", kind: "chequing", opening_balance: 100_000, archived: false, created_at: 0, updated_at: 0 },
    { id: ID(2), name: "Savings", kind: "savings", opening_balance: 0, archived: false, created_at: 0, updated_at: 0 },
  ];
  b.categories = [
    { id: ID(10), name: "Groceries", kind: "expense", color: "#2c7445", monthly_budget: 40_000, created_at: 0, updated_at: 0 },
    { id: ID(11), name: "Salary", kind: "income", color: "#3b3fa8", monthly_budget: null, created_at: 0, updated_at: 0 },
  ];
  b.transactions = [
    txn({ id: ID(20), category_id: ID(10), amount: 2_550, payee: "Market" }),
    txn({ id: ID(21), kind: "income", category_id: ID(11), amount: 300_000, date: "2026-11-01", payee: "Work" }),
    txn({ id: ID(22), kind: "transfer", to_account_id: ID(2), amount: 50_000, date: "2026-11-03" }),
    txn({ id: ID(23), amount: 1_000, date: "2026-10-15", payee: "Cafe" }),
  ];
  return b;
}

describe("money", () => {
  it("parses typed and imported amounts into minor units", () => {
    expect(parseMoney("12", "CAD")).toBe(1200);
    expect(parseMoney("12.5", "CAD")).toBe(1250);
    expect(parseMoney("$1,234.56", "CAD")).toBe(123456);
    expect(parseMoney("-4.20", "CAD")).toBe(-420);
    expect(parseMoney("(4.20)", "CAD")).toBe(-420);
    expect(parseMoney(".5", "CAD")).toBe(50);
    expect(parseMoney("CAD 3.00", "CAD")).toBe(300);
    expect(parseMoney("1.234", "CAD")).toBeUndefined();
    expect(parseMoney("abc", "CAD")).toBeUndefined();
    expect(parseMoney("500", "JPY")).toBe(500);
    expect(parseMoney("5.5", "JPY")).toBeUndefined();
  });

  it("formats and round-trips decimals", () => {
    expect(toDecimal(123456, "CAD")).toBe("1234.56");
    expect(toDecimal(-5, "CAD")).toBe("-0.05");
    expect(toDecimal(500, "JPY")).toBe("500");
    expect(formatMoney(123456, "CAD")).toContain("1,234.56");
  });
});

describe("figures", () => {
  it("derives balances from opening balances and transfers", () => {
    const b = balances(sample());
    expect(b.get(ID(1))).toBe(100_000 - 2_550 + 300_000 - 50_000 - 1_000);
    expect(b.get(ID(2))).toBe(50_000);
  });

  it("leaves transfers out of income and spending", () => {
    const t = totals(sample().transactions, presetRange("this-month", "2026-11-10"));
    expect(t).toEqual({ income: 300_000, spending: 2_550, net: 297_450, count: 2 });
    expect(totals(sample().transactions, {}).spending).toBe(3_550);
  });

  it("groups spending by category with uncategorized last when smaller", () => {
    const rows = byCategory(sample(), {});
    expect(rows.map((r) => [r.name, r.amount])).toEqual([
      ["Groceries", 2_550],
      ["Uncategorized", 1_000],
    ]);
  });

  it("compares budgets with the month's spending", () => {
    const [line] = budgetLines(sample(), "2026-11");
    expect(line).toMatchObject({ budget: 40_000, spent: 2_550, left: 37_450 });
  });

  it("totals months oldest first", () => {
    const m = monthlyTotals(sample().transactions, "2026-11", 3);
    expect(m.map((x) => x.month)).toEqual(["2026-09", "2026-10", "2026-11"]);
    expect(m[1].spending).toBe(1_000);
  });

  it("presets cover the right dates", () => {
    expect(presetRange("last-month", "2026-01-15")).toEqual({ from: "2025-12-01", to: "2025-12-31" });
    expect(presetRange("3-months", "2026-03-02")).toEqual({ from: "2026-01-01", to: "2026-03-31" });
  });
});

describe("recurring", () => {
  it("clamps month ends to the anchor day", () => {
    expect(nextOccurrence("2026-01-31", "monthly", 31)).toBe("2026-02-28");
    expect(nextOccurrence("2026-02-28", "monthly", 31)).toBe("2026-03-31");
    expect(nextOccurrence("2028-02-29", "yearly", 29)).toBe("2029-02-28");
    expect(nextOccurrence("2026-12-30", "biweekly", 30)).toBe("2027-01-13");
  });

  it("lists pending occurrences and only the next is postable", () => {
    const b = sample();
    b.templates = [
      { id: ID(30), kind: "expense", account_id: ID(1), to_account_id: null, category_id: null, amount: 1_000, payee: "Gym", cadence: "weekly", next_date: "2026-11-01", anchor_day: 1, active: true, created_at: 0, updated_at: 0 },
      { id: ID(31), kind: "expense", account_id: ID(1), to_account_id: null, category_id: null, amount: 1_000, payee: "Paused", cadence: "weekly", next_date: "2026-11-01", anchor_day: 1, active: false, created_at: 0, updated_at: 0 },
    ];
    const occ = upcoming(b.templates, "2026-11-05", 14);
    expect(occ.map((o) => [o.date, o.due, o.postable])).toEqual([
      ["2026-11-01", true, true],
      ["2026-11-08", false, false],
      ["2026-11-15", false, false],
    ]);
    const posted = postLocally(b, ID(30), ID(40), 5);
    expect(posted.transactions.find((t) => t.id === ID(40))).toMatchObject({ date: "2026-11-01", template_id: ID(30), amount: 1_000 });
    expect(posted.templates[0].next_date).toBe("2026-11-08");
    expect(postLocally(posted, ID(30), ID(40), 6)).toBe(posted);
  });
});

describe("local changes", () => {
  it("deleting a category keeps its transactions, uncategorized", () => {
    const b = removeRecord(sample(), "categories", ID(10));
    expect(b.categories).toHaveLength(1);
    expect(b.transactions.find((t) => t.id === ID(20))?.category_id).toBeNull();
  });

  it("imports skip ids already present", () => {
    const b = sample();
    const row = { id: ID(20), kind: "expense" as const, account_id: ID(1), to_account_id: null, category_id: null, amount: 5, date: "2026-11-04", payee: "", note: "" };
    expect(importLocally(b, [row, { ...row, id: ID(50) }], 1).transactions).toHaveLength(5);
  });
});

describe("csv", () => {
  it("parses quotes, commas, and line breaks", () => {
    expect(parseCsv('a,"b, c","say ""hi"""\r\n\r\n1,2,"x\ny"')).toEqual([
      ["a", "b, c", 'say "hi"'],
      ["1", "2", "x\ny"],
    ]);
  });

  it("exports with names and guards spreadsheet formulas", () => {
    const b = sample();
    b.transactions[0].payee = "=HYPERLINK()";
    const lines = toCsv(b).trim().split("\r\n");
    expect(lines[0]).toBe("date,type,amount,account,to_account,category,payee,note,id");
    expect(lines).toContain(`2026-11-02,expense,25.50,Chequing,,Groceries,'=HYPERLINK(),,${ID(20)}`);
    expect(lines).toContain(`2026-11-03,transfer,500.00,Chequing,Savings,,,,${ID(22)}`);
  });

  it("round-trips an export as duplicates", () => {
    const b = sample();
    const { rows } = previewImport(toCsv(b), b, ID(1), () => ID(99));
    expect(rows.every((r) => r.input && r.duplicate)).toBe(true);
  });

  it("previews a bank file: signs, debit/credit, errors, and duplicates", () => {
    const b = sample();
    let n = 100;
    const next = () => ID(n++);
    const signed = previewImport("Date,Description,Amount,Category\n2026-11-04,Bakery,-3.50,Groceries\n2026-11-05,Refund,12.00,\n11/05/2026,Bad,1\n2026-11-02,Market,-25.50,\n2026-11-04,Bakery,-3.50,Groceries\n2026-11-06,Odd,-1,Fun", b, ID(1), next);
    expect(signed.rows.map((r) => [r.line, r.input?.kind, r.input?.amount, r.duplicate, !!r.error])).toEqual([
      [2, "expense", 350, false, false],
      [3, "income", 1_200, false, false],
      [4, undefined, undefined, false, true],
      [5, "expense", 2_550, true, false],
      [6, "expense", 350, true, false],
      [7, "expense", 100, false, false],
    ]);
    expect(signed.rows[0].input?.category_id).toBe(ID(10));
    expect(signed.rows[5].unknownCategory).toBe("Fun");

    const split = previewImport("Posted,Details,Debit,Credit\n2026/11/07,Shop,4.00,\n2026/11/08,Pay,,10.00", b, ID(2), next);
    expect(split.rows.map((r) => [r.input?.date, r.input?.kind, r.input?.amount, r.input?.account_id])).toEqual([
      ["2026-11-07", "expense", 400, ID(2)],
      ["2026-11-08", "income", 1_000, ID(2)],
    ]);
    expect(previewImport("x,y\n1,2", b, ID(1), next).error).toMatch(/column names/);
  });
});
