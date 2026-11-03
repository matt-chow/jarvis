//! D1 implementation of `BudgetRepository` (migration 0028).

use super::{changes, err, int, opt_int, opt_text, stmt, text};
use directory_core::budget::{
    Account, Budget, BudgetCategory, BudgetRepository, Entity, Flow, Item, Template, Transaction,
    DEFAULT_CURRENCY,
};
use serde::Deserialize;
use worker::wasm_bindgen::JsValue;
use worker::{D1Database, D1PreparedStatement};

pub struct D1BudgetRepository {
    db: D1Database,
}

impl D1BudgetRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }
}

const ACCOUNT: &str = "id, owner_id, name, kind, opening_balance, archived, created_at, updated_at";
const CATEGORY: &str = "id, owner_id, name, kind, color, monthly_budget, created_at, updated_at";
const TRANSACTION: &str = "id, owner_id, kind, account_id, to_account_id, category_id, amount, date, payee, note, template_id, created_at, updated_at";
const TEMPLATE: &str = "id, owner_id, kind, account_id, to_account_id, category_id, amount, payee, cadence, next_date, anchor_day, active, created_at, updated_at";

fn table(e: Entity) -> (&'static str, &'static str) {
    match e {
        Entity::Account => ("budget_accounts", ACCOUNT),
        Entity::Category => ("budget_categories", CATEGORY),
        Entity::Transaction => ("budget_transactions", TRANSACTION),
        Entity::Template => ("budget_templates", TEMPLATE),
    }
}

fn flow(s: &str) -> Flow {
    Flow::parse(s).unwrap_or(Flow::Expense)
}

#[derive(Deserialize)]
struct AccountRow {
    id: String,
    owner_id: String,
    name: String,
    kind: String,
    opening_balance: f64,
    archived: f64,
    created_at: f64,
    updated_at: f64,
}

impl From<AccountRow> for Account {
    fn from(r: AccountRow) -> Self {
        Account {
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            kind: r.kind,
            opening_balance: r.opening_balance as i64,
            archived: r.archived != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct CategoryRow {
    id: String,
    owner_id: String,
    name: String,
    kind: String,
    color: String,
    monthly_budget: Option<f64>,
    created_at: f64,
    updated_at: f64,
}

impl From<CategoryRow> for BudgetCategory {
    fn from(r: CategoryRow) -> Self {
        BudgetCategory {
            id: r.id,
            owner_id: r.owner_id,
            name: r.name,
            kind: flow(&r.kind),
            color: r.color,
            monthly_budget: r.monthly_budget.map(|v| v as i64),
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct TransactionRow {
    id: String,
    owner_id: String,
    kind: String,
    account_id: String,
    to_account_id: Option<String>,
    category_id: Option<String>,
    amount: f64,
    date: String,
    payee: String,
    note: String,
    template_id: Option<String>,
    created_at: f64,
    updated_at: f64,
}

impl From<TransactionRow> for Transaction {
    fn from(r: TransactionRow) -> Self {
        Transaction {
            id: r.id,
            owner_id: r.owner_id,
            kind: flow(&r.kind),
            account_id: r.account_id,
            to_account_id: r.to_account_id,
            category_id: r.category_id,
            amount: r.amount as i64,
            date: r.date,
            payee: r.payee,
            note: r.note,
            template_id: r.template_id,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct TemplateRow {
    id: String,
    owner_id: String,
    kind: String,
    account_id: String,
    to_account_id: Option<String>,
    category_id: Option<String>,
    amount: f64,
    payee: String,
    cadence: String,
    next_date: String,
    anchor_day: f64,
    active: f64,
    created_at: f64,
    updated_at: f64,
}

impl From<TemplateRow> for Template {
    fn from(r: TemplateRow) -> Self {
        Template {
            id: r.id,
            owner_id: r.owner_id,
            kind: flow(&r.kind),
            account_id: r.account_id,
            to_account_id: r.to_account_id,
            category_id: r.category_id,
            amount: r.amount as i64,
            payee: r.payee,
            cadence: r.cadence,
            next_date: r.next_date,
            anchor_day: r.anchor_day as u32,
            active: r.active != 0.0,
            created_at: r.created_at as i64,
            updated_at: r.updated_at as i64,
        }
    }
}

fn flag(b: bool) -> JsValue {
    int(i64::from(b))
}

/// Column values for an item, in its column order (owner second).
fn values(owner_id: &str, item: &Item) -> Vec<JsValue> {
    match item {
        Item::Account(a) => vec![
            text(&a.id),
            text(owner_id),
            text(&a.name),
            text(&a.kind),
            int(a.opening_balance),
            flag(a.archived),
            int(a.created_at),
            int(a.updated_at),
        ],
        Item::Category(c) => vec![
            text(&c.id),
            text(owner_id),
            text(&c.name),
            text(c.kind.as_str()),
            text(&c.color),
            opt_int(c.monthly_budget),
            int(c.created_at),
            int(c.updated_at),
        ],
        Item::Transaction(t) => vec![
            text(&t.id),
            text(owner_id),
            text(t.kind.as_str()),
            text(&t.account_id),
            opt_text(t.to_account_id.as_deref()),
            opt_text(t.category_id.as_deref()),
            int(t.amount),
            text(&t.date),
            text(&t.payee),
            text(&t.note),
            opt_text(t.template_id.as_deref()),
            int(t.created_at),
            int(t.updated_at),
        ],
        Item::Template(t) => vec![
            text(&t.id),
            text(owner_id),
            text(t.kind.as_str()),
            text(&t.account_id),
            opt_text(t.to_account_id.as_deref()),
            opt_text(t.category_id.as_deref()),
            int(t.amount),
            text(&t.payee),
            text(&t.cadence),
            text(&t.next_date),
            int(i64::from(t.anchor_day)),
            flag(t.active),
            int(t.created_at),
            int(t.updated_at),
        ],
    }
}

fn entity(item: &Item) -> Entity {
    match item {
        Item::Account(_) => Entity::Account,
        Item::Category(_) => Entity::Category,
        Item::Transaction(_) => Entity::Transaction,
        Item::Template(_) => Entity::Template,
    }
}

fn placeholders(n: usize) -> String {
    (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl D1BudgetRepository {
    fn insert_stmt(
        &self,
        owner_id: &str,
        item: &Item,
        or_ignore: bool,
    ) -> Result<D1PreparedStatement, String> {
        let (table, columns) = table(entity(item));
        let args = values(owner_id, item);
        let verb = if or_ignore {
            "INSERT OR IGNORE"
        } else {
            "INSERT"
        };
        let sql = format!(
            "{verb} INTO {table} ({columns}) VALUES ({})",
            placeholders(args.len())
        );
        stmt(&self.db, &sql, &args)
    }

    /// Rewrites every column but id, owner, and created_at.
    fn replace_stmt(&self, owner_id: &str, item: &Item) -> Result<D1PreparedStatement, String> {
        let (table, columns) = table(entity(item));
        let args = values(owner_id, item);
        let sets: Vec<String> = columns
            .split(", ")
            .enumerate()
            .filter(|(_, c)| !matches!(*c, "id" | "owner_id" | "created_at"))
            .map(|(i, c)| format!("{c} = ?{}", i + 1))
            .collect();
        let sql = format!(
            "UPDATE {table} SET {} WHERE id = ?1 AND owner_id = ?2",
            sets.join(", ")
        );
        stmt(&self.db, &sql, &args)
    }

    async fn rows<T: for<'de> Deserialize<'de>>(
        &self,
        e: Entity,
        owner_id: &str,
        order: &str,
    ) -> Result<Vec<T>, String> {
        let (table, columns) = table(e);
        let sql = format!("SELECT {columns} FROM {table} WHERE owner_id = ?1 ORDER BY {order}");
        stmt(&self.db, &sql, &[text(owner_id)])?
            .all()
            .await
            .map_err(err)?
            .results::<T>()
            .map_err(err)
    }
}

impl BudgetRepository for D1BudgetRepository {
    async fn load(&self, owner_id: &str, transactions: bool) -> Result<Budget, String> {
        #[derive(Deserialize)]
        struct Cur {
            currency: String,
        }
        let currency = stmt(
            &self.db,
            "SELECT currency FROM budget_settings WHERE owner_id = ?1",
            &[text(owner_id)],
        )?
        .first::<Cur>(None)
        .await
        .map_err(err)?
        .map_or(DEFAULT_CURRENCY.into(), |c| c.currency);
        Ok(Budget {
            currency,
            accounts: self
                .rows::<AccountRow>(Entity::Account, owner_id, "created_at")
                .await?
                .into_iter()
                .map(Account::from)
                .collect(),
            categories: self
                .rows::<CategoryRow>(Entity::Category, owner_id, "name COLLATE NOCASE")
                .await?
                .into_iter()
                .map(BudgetCategory::from)
                .collect(),
            transactions: if transactions {
                self.rows::<TransactionRow>(
                    Entity::Transaction,
                    owner_id,
                    "date DESC, created_at DESC",
                )
                .await?
                .into_iter()
                .map(Transaction::from)
                .collect()
            } else {
                Vec::new()
            },
            templates: self
                .rows::<TemplateRow>(Entity::Template, owner_id, "next_date")
                .await?
                .into_iter()
                .map(Template::from)
                .collect(),
        })
    }

    async fn set_currency(&self, owner_id: &str, currency: &str, now: i64) -> Result<(), String> {
        stmt(
            &self.db,
            "INSERT INTO budget_settings (owner_id, currency, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (owner_id) DO UPDATE SET currency = ?2, updated_at = ?3",
            &[text(owner_id), text(currency), int(now)],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(())
    }

    async fn get(&self, owner_id: &str, e: Entity, id: &str) -> Result<Option<Item>, String> {
        let (table, columns) = table(e);
        let sql = format!("SELECT {columns} FROM {table} WHERE owner_id = ?1 AND id = ?2");
        let q = stmt(&self.db, &sql, &[text(owner_id), text(id)])?;
        Ok(match e {
            Entity::Account => q
                .first::<AccountRow>(None)
                .await
                .map_err(err)?
                .map(|r| Item::Account(r.into())),
            Entity::Category => q
                .first::<CategoryRow>(None)
                .await
                .map_err(err)?
                .map(|r| Item::Category(r.into())),
            Entity::Transaction => q
                .first::<TransactionRow>(None)
                .await
                .map_err(err)?
                .map(|r| Item::Transaction(r.into())),
            Entity::Template => q
                .first::<TemplateRow>(None)
                .await
                .map_err(err)?
                .map(|r| Item::Template(r.into())),
        })
    }

    async fn transaction_count(&self, owner_id: &str) -> Result<u32, String> {
        super::count(
            &self.db,
            "SELECT COUNT(*) AS n FROM budget_transactions WHERE owner_id = ?1",
            &[text(owner_id)],
        )
        .await
    }

    async fn account_in_use(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        Ok(super::count(
            &self.db,
            "SELECT (SELECT COUNT(*) FROM budget_transactions WHERE owner_id = ?1 AND (account_id = ?2 OR to_account_id = ?2))
                  + (SELECT COUNT(*) FROM budget_templates WHERE owner_id = ?1 AND (account_id = ?2 OR to_account_id = ?2)) AS n",
            &[text(owner_id), text(id)],
        )
        .await?
            > 0)
    }

    async fn insert(&self, owner_id: &str, item: &Item) -> Result<(), String> {
        self.insert_stmt(owner_id, item, false)?
            .run()
            .await
            .map_err(err)?;
        Ok(())
    }

    async fn replace(&self, owner_id: &str, item: &Item) -> Result<bool, String> {
        let r = self
            .replace_stmt(owner_id, item)?
            .run()
            .await
            .map_err(err)?;
        Ok(changes(&r) > 0)
    }

    async fn delete(&self, owner_id: &str, e: Entity, id: &str) -> Result<bool, String> {
        let (table, _) = table(e);
        let mut batch = Vec::new();
        if e == Entity::Category {
            for t in ["budget_transactions", "budget_templates"] {
                batch.push(stmt(
                    &self.db,
                    &format!("UPDATE {t} SET category_id = NULL WHERE owner_id = ?1 AND category_id = ?2"),
                    &[text(owner_id), text(id)],
                )?);
            }
        }
        batch.push(stmt(
            &self.db,
            &format!("DELETE FROM {table} WHERE owner_id = ?1 AND id = ?2"),
            &[text(owner_id), text(id)],
        )?);
        let results = self.db.batch(batch).await.map_err(err)?;
        Ok(results.last().is_some_and(|r| changes(r) > 0))
    }

    async fn post(
        &self,
        owner_id: &str,
        txn: &Transaction,
        template: &Template,
    ) -> Result<(), String> {
        self.db
            .batch(vec![
                self.insert_stmt(owner_id, &Item::Transaction(txn.clone()), false)?,
                self.replace_stmt(owner_id, &Item::Template(template.clone()))?,
            ])
            .await
            .map_err(err)?;
        Ok(())
    }

    async fn insert_many(&self, owner_id: &str, txns: &[Transaction]) -> Result<u32, String> {
        let mut inserted = 0;
        // D1 batches run as one transaction; keep each batch moderate.
        for chunk in txns.chunks(100) {
            let batch = chunk
                .iter()
                .map(|t| self.insert_stmt(owner_id, &Item::Transaction(t.clone()), true))
                .collect::<Result<Vec<_>, _>>()?;
            let results = self.db.batch(batch).await.map_err(err)?;
            inserted += results.iter().filter(|r| changes(r) > 0).count() as u32;
        }
        Ok(inserted)
    }
}
