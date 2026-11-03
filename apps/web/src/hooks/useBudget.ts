import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import {
  IMPORT_CHUNK,
  importLocally,
  postLocally,
  recordFrom,
  removeRecord,
  upsertRecord,
  type BudgetData,
  type Entity,
  type Inputs,
  type Records,
  type TransactionInput,
} from "../lib/budget";
import { clientId, type Op } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface BudgetState {
  data?: BudgetData;
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  save: <E extends Entity>(entity: E, input: Inputs[E], id?: string) => Promise<Records[E] | undefined>;
  remove: (entity: Entity, id: string) => Promise<boolean>;
  /** Record a recurring item's next occurrence. */
  post: (templateId: string) => Promise<boolean>;
  setCurrency: (currency: string) => Promise<boolean>;
  /** Store rows in chunks; returns how many were new (offline: queued). */
  importRows: (rows: (TransactionInput & { id: string })[]) => Promise<{ imported: number; skipped: number } | undefined>;
}

// The budget is one document; the cached list holds it as its only item.
const fetchAll = async () => [await api.budget()];

/** Apply a queued budget op to the cached budget. */
export function applyBudgetLocally(items: BudgetData[], op: Op, now = Date.now()): BudgetData[] {
  const [data] = items;
  if (!data) return items;
  switch (op.kind) {
    case "budget.create":
    case "budget.update": {
      const old = (data[op.entity] as { id: string }[]).find((x) => x.id === op.id) as Records[Entity] | undefined;
      return [upsertRecord(data, op.entity, recordFrom(op.entity, op.id, op.input, now, old))];
    }
    case "budget.delete":
      return [removeRecord(data, op.entity, op.id)];
    case "budget.post":
      return [postLocally(data, op.template_id, op.txn_id, now)];
    case "budget.currency":
      return [{ ...data, currency: op.currency.toUpperCase() }];
    case "budget.import":
      return [importLocally(data, op.transactions, now)];
    default:
      return items;
  }
}

const applyLocal = (cur: BudgetData[], op: Op) => applyBudgetLocally(cur, op);
const withRecord = <E extends Entity>(cur: BudgetData[], entity: E, r: Records[E]) => (cur[0] ? [upsertRecord(cur[0], entity, r)] : cur);

/** The owner's budget (D-050); every change works offline and syncs later. */
export function useBudget(online: boolean): BudgetState {
  const list = useCachedList<BudgetData>("budget:last", "budget item", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const save = useCallback(
    <E extends Entity>(entity: E, input: Inputs[E], existing?: string) => {
      const id = existing ?? clientId();
      const op: Op = existing ? { kind: "budget.update", entity, id, input } : { kind: "budget.create", entity, id, input };
      return runOrQueue<BudgetData, Records[E]>({
        list,
        noun: "budget item",
        op,
        server: () => (existing ? api.replaceBudget(entity, id, input) : api.createBudget(entity, id, input)),
        fromServer: (cur, r) => withRecord(cur, entity, r),
        applyLocal,
        local: (cur) => (cur[0]?.[entity] as Records[E][] | undefined)?.find((x) => x.id === id) as Records[E],
      });
    },
    [list],
  );

  const remove = useCallback(
    async (entity: Entity, id: string) =>
      (await runOrQueue({
        list,
        noun: "budget item",
        op: { kind: "budget.delete", entity, id },
        server: async () => {
          await api.deleteBudget(entity, id);
          return true;
        },
        fromServer: (cur) => (cur[0] ? [removeRecord(cur[0], entity, id)] : cur),
        applyLocal,
        local: () => true,
      })) ?? false,
    [list],
  );

  const post = useCallback(
    async (templateId: string) => {
      const txnId = clientId();
      return (
        (await runOrQueue({
          list,
          noun: "recurring item",
          op: { kind: "budget.post", template_id: templateId, txn_id: txnId },
          server: () => api.postTemplate(templateId, txnId),
          fromServer: (cur, r) => (cur[0] ? [upsertRecord(upsertRecord(cur[0], "transactions", r.transaction), "templates", r.template)] : cur),
          applyLocal,
          local: (cur) => ({ transaction: cur[0]?.transactions.find((t) => t.id === txnId)!, template: cur[0]?.templates.find((t) => t.id === templateId)! }),
        })) !== undefined
      );
    },
    [list],
  );

  const setCurrency = useCallback(
    async (currency: string) =>
      (await runOrQueue({
        list,
        noun: "currency",
        op: { kind: "budget.currency", currency },
        server: () => api.setBudgetCurrency(currency),
        fromServer: (cur, r) => (cur[0] ? [{ ...cur[0], currency: r.currency }] : cur),
        applyLocal,
        local: () => ({ currency }),
      })) !== undefined,
    [list],
  );

  const importRows = useCallback(
    async (rows: (TransactionInput & { id: string })[]) => {
      let imported = 0;
      let skipped = 0;
      for (let i = 0; i < rows.length; i += IMPORT_CHUNK) {
        const chunk = rows.slice(i, i + IMPORT_CHUNK);
        const r = await runOrQueue({
          list,
          noun: "import",
          op: { kind: "budget.import", transactions: chunk },
          server: () => api.importBudget(chunk),
          fromServer: (cur) => applyLocal(cur, { kind: "budget.import", transactions: chunk }),
          applyLocal,
          local: () => ({ imported: chunk.length, skipped: 0 }),
        });
        if (!r) return i === 0 ? undefined : { imported, skipped };
        imported += r.imported;
        skipped += r.skipped;
      }
      return { imported, skipped };
    },
    [list],
  );

  return { data: list.items?.[0], loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, save, remove, post, setCurrency, importRows };
}
