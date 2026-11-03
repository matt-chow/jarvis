//! Budget (D-050): accounts with opening balances, transactions (expense,
//! income, transfer), categories with an optional monthly budget, and
//! recurring templates whose due occurrences are posted on request.
//!
//! Amounts are integer minor units (cents) in one configurable currency.
//! The server validates and stores the records, scoped to the owner;
//! balances and reports are derived from them (on the client, so they work
//! offline). No bank connections.

use crate::entropy::{is_id, new_id, Entropy};
use crate::identity::Principal;
use crate::validate;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Largest single amount or opening balance: one billion major units.
pub const MAX_AMOUNT: i64 = 100_000_000_000;
pub const MAX_NAME: usize = 60;
pub const MAX_PAYEE: usize = 120;
pub const MAX_NOTE: usize = 1000;
pub const MAX_ACCOUNTS: usize = 50;
pub const MAX_CATEGORIES: usize = 200;
pub const MAX_TEMPLATES: usize = 200;
pub const MAX_TRANSACTIONS: u32 = 10_000;
/// Most transactions one import request may carry.
pub const MAX_IMPORT: usize = 1000;
pub const DEFAULT_CURRENCY: &str = "CAD";
pub const DEFAULT_COLOR: &str = "#3b3fa8";
pub const ACCOUNT_KINDS: [&str; 5] = ["cash", "chequing", "savings", "credit", "other"];
pub const CADENCES: [&str; 4] = ["weekly", "biweekly", "monthly", "yearly"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Flow {
    Expense,
    Income,
    Transfer,
}

impl Flow {
    pub fn as_str(self) -> &'static str {
        match self {
            Flow::Expense => "expense",
            Flow::Income => "income",
            Flow::Transfer => "transfer",
        }
    }

    pub fn parse(s: &str) -> Option<Flow> {
        match s {
            "expense" => Some(Flow::Expense),
            "income" => Some(Flow::Income),
            "transfer" => Some(Flow::Transfer),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    /// One of [`ACCOUNT_KINDS`].
    pub kind: String,
    /// Balance before the first recorded transaction (may be negative, e.g. a credit card).
    pub opening_balance: i64,
    pub archived: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetCategory {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub name: String,
    /// `expense` or `income`.
    pub kind: Flow,
    pub color: String,
    /// Monthly budget for an expense category, if set.
    pub monthly_budget: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub kind: Flow,
    pub account_id: String,
    /// The receiving account of a transfer.
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    /// Always positive; the kind gives the direction.
    pub amount: i64,
    pub date: String,
    pub payee: String,
    pub note: String,
    /// The recurring template this was posted from, if any.
    pub template_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub id: String,
    #[serde(skip)]
    pub owner_id: String,
    pub kind: Flow,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub category_id: Option<String>,
    pub amount: i64,
    pub payee: String,
    /// One of [`CADENCES`].
    pub cadence: String,
    /// The next occurrence, pending until posted.
    pub next_date: String,
    /// Day of the month monthly and yearly occurrences fall on (clamped to the month's length).
    pub anchor_day: u32,
    pub active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub opening_balance: i64,
    #[serde(default)]
    pub archived: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CategoryInput {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "expense")]
    pub kind: Flow,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub monthly_budget: Option<i64>,
}

fn expense() -> Flow {
    Flow::Expense
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransactionInput {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: Flow,
    pub account_id: String,
    #[serde(default)]
    pub to_account_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    pub amount: i64,
    pub date: String,
    #[serde(default)]
    pub payee: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TemplateInput {
    #[serde(default)]
    pub id: Option<String>,
    pub kind: Flow,
    pub account_id: String,
    #[serde(default)]
    pub to_account_id: Option<String>,
    #[serde(default)]
    pub category_id: Option<String>,
    pub amount: i64,
    #[serde(default)]
    pub payee: String,
    pub cadence: String,
    pub next_date: String,
    #[serde(default = "yes")]
    pub active: bool,
}

fn yes() -> bool {
    true
}

/// Everything the owner has (`GET /budget`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub currency: String,
    pub accounts: Vec<Account>,
    pub categories: Vec<BudgetCategory>,
    pub transactions: Vec<Transaction>,
    pub templates: Vec<Template>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entity {
    Account,
    Category,
    Transaction,
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Account(Account),
    Category(BudgetCategory),
    Transaction(Transaction),
    Template(Template),
}

impl Item {
    pub fn id(&self) -> &str {
        match self {
            Item::Account(a) => &a.id,
            Item::Category(c) => &c.id,
            Item::Transaction(t) => &t.id,
            Item::Template(t) => &t.id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    Invalid(&'static str, &'static str),
    /// A client-supplied id is already in use (a replayed offline create).
    Exists,
    NotFound,
    Backend(String),
}

/// What an import did: new transactions stored, and ids already present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Imported {
    pub imported: u32,
    pub skipped: u32,
}

pub trait BudgetRepository {
    /// The owner's records. Without `transactions`, the list is left empty
    /// (writes validate against accounts and categories only).
    async fn load(&self, owner_id: &str, transactions: bool) -> Result<Budget, String>;
    async fn set_currency(&self, owner_id: &str, currency: &str, now: i64) -> Result<(), String>;
    async fn get(&self, owner_id: &str, entity: Entity, id: &str) -> Result<Option<Item>, String>;
    async fn transaction_count(&self, owner_id: &str) -> Result<u32, String>;
    /// Transactions or templates that use the account.
    async fn account_in_use(&self, owner_id: &str, id: &str) -> Result<bool, String>;
    async fn insert(&self, owner_id: &str, item: &Item) -> Result<(), String>;
    async fn replace(&self, owner_id: &str, item: &Item) -> Result<bool, String>;
    /// Delete one record. Deleting a category clears it from transactions and
    /// templates in the same batch.
    async fn delete(&self, owner_id: &str, entity: Entity, id: &str) -> Result<bool, String>;
    /// Store a template's occurrence and move the template on, atomically.
    async fn post(
        &self,
        owner_id: &str,
        txn: &Transaction,
        template: &Template,
    ) -> Result<(), String>;
    /// Store transactions, skipping ids already present; returns how many were stored.
    async fn insert_many(&self, owner_id: &str, txns: &[Transaction]) -> Result<u32, String>;
}

fn backend(e: String) -> BudgetError {
    BudgetError::Backend(e)
}

/// Three ASCII letters, upper-cased (ISO 4217 shape; not checked against a list).
pub fn currency(raw: &str) -> Result<String, BudgetError> {
    let s = raw.trim().to_ascii_uppercase();
    if s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(s)
    } else {
        Err(BudgetError::Invalid("currency", "expected a 3-letter code"))
    }
}

fn amount(field: &'static str, v: i64, positive: bool) -> Result<i64, BudgetError> {
    if positive && v <= 0 {
        return Err(BudgetError::Invalid(field, "must be more than zero"));
    }
    if v.abs() > MAX_AMOUNT {
        return Err(BudgetError::Invalid(field, "too large"));
    }
    Ok(v)
}

fn name(raw: &str) -> Result<String, BudgetError> {
    validate::line(raw, MAX_NAME).map_err(|w| BudgetError::Invalid("name", w))
}

fn payee(raw: &str) -> Result<String, BudgetError> {
    match raw.trim() {
        "" => Ok(String::new()),
        s => validate::line(s, MAX_PAYEE).map_err(|w| BudgetError::Invalid("payee", w)),
    }
}

fn color(raw: Option<&str>) -> Result<String, BudgetError> {
    crate::categories::color(raw.unwrap_or(DEFAULT_COLOR))
        .map_err(|_| BudgetError::Invalid("color", "expected #rrggbb"))
}

fn date(field: &'static str, raw: &str) -> Result<String, BudgetError> {
    validate::date(raw).map_err(|w| BudgetError::Invalid(field, w))
}

fn days_in_month(y: i64, m: i64) -> i64 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// The occurrence after `date` for a cadence; monthly and yearly ones fall
/// on `anchor_day`, clamped to the month's length (31 → Feb 28 → Mar 31).
pub fn next_occurrence(date: &str, cadence: &str, anchor_day: u32) -> String {
    let months = match cadence {
        "weekly" => return validate::add_days(date, 7),
        "biweekly" => return validate::add_days(date, 14),
        "yearly" => 12,
        _ => 1,
    };
    let n = |r: std::ops::Range<usize>| date[r].parse::<i64>().unwrap_or(1);
    let total = n(0..4) * 12 + (n(5..7) - 1) + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) + 1);
    let d = i64::from(anchor_day.clamp(1, 31)).min(days_in_month(y, m));
    format!("{y:04}-{m:02}-{d:02}")
}

fn day_of(date: &str) -> u32 {
    date[8..10].parse().unwrap_or(1)
}

/// Checks a flow's accounts and category against the owner's records.
fn check_flow(
    b: &Budget,
    kind: Flow,
    account_id: &str,
    to_account_id: Option<&str>,
    category_id: Option<&str>,
) -> Result<(), BudgetError> {
    let known = |id: &str| b.accounts.iter().any(|a| a.id == id);
    if !known(account_id) {
        return Err(BudgetError::Invalid("account_id", "unknown account"));
    }
    match kind {
        Flow::Transfer => {
            let to = to_account_id.ok_or(BudgetError::Invalid(
                "to_account_id",
                "required for a transfer",
            ))?;
            if !known(to) {
                return Err(BudgetError::Invalid("to_account_id", "unknown account"));
            }
            if to == account_id {
                return Err(BudgetError::Invalid(
                    "to_account_id",
                    "must differ from the account",
                ));
            }
            if category_id.is_some() {
                return Err(BudgetError::Invalid(
                    "category_id",
                    "transfers have no category",
                ));
            }
        }
        _ => {
            if to_account_id.is_some() {
                return Err(BudgetError::Invalid(
                    "to_account_id",
                    "only transfers have a receiving account",
                ));
            }
            if let Some(c) = category_id {
                let cat = b
                    .categories
                    .iter()
                    .find(|x| x.id == c)
                    .ok_or(BudgetError::Invalid("category_id", "unknown category"))?;
                if cat.kind != kind {
                    return Err(BudgetError::Invalid(
                        "category_id",
                        "category is for the other direction",
                    ));
                }
            }
        }
    }
    Ok(())
}

pub struct BudgetService<'a, R, E: ?Sized> {
    repo: &'a R,
    entropy: &'a E,
    now: i64,
}

impl<'a, R: BudgetRepository, E: Entropy + ?Sized> BudgetService<'a, R, E> {
    pub fn new(repo: &'a R, entropy: &'a E, now: i64) -> Self {
        Self { repo, entropy, now }
    }

    pub async fn load(&self, p: &Principal) -> Result<Budget, BudgetError> {
        self.repo.load(&p.user_id, true).await.map_err(backend)
    }

    async fn refs(&self, p: &Principal) -> Result<Budget, BudgetError> {
        self.repo.load(&p.user_id, false).await.map_err(backend)
    }

    pub async fn set_currency(&self, p: &Principal, raw: &str) -> Result<String, BudgetError> {
        let c = currency(raw)?;
        self.repo
            .set_currency(&p.user_id, &c, self.now)
            .await
            .map_err(backend)?;
        Ok(c)
    }

    /// A fresh id for a create: the client's (checked unused) or a new one.
    async fn fresh_id(
        &self,
        p: &Principal,
        entity: Entity,
        id: Option<String>,
    ) -> Result<String, BudgetError> {
        match id {
            Some(id) if !is_id(&id) => {
                Err(BudgetError::Invalid("id", "expected 32 hex characters"))
            }
            Some(id) => {
                if self
                    .repo
                    .get(&p.user_id, entity, &id)
                    .await
                    .map_err(backend)?
                    .is_some()
                {
                    Err(BudgetError::Exists)
                } else {
                    Ok(id)
                }
            }
            None => Ok(new_id(self.entropy)),
        }
    }

    /// The existing record for a replace (its id and creation time are kept).
    async fn existing(&self, p: &Principal, entity: Entity, id: &str) -> Result<Item, BudgetError> {
        if !is_id(id) {
            return Err(BudgetError::NotFound);
        }
        self.repo
            .get(&p.user_id, entity, id)
            .await
            .map_err(backend)?
            .ok_or(BudgetError::NotFound)
    }

    fn account(
        &self,
        p: &Principal,
        b: &Budget,
        id: String,
        created_at: i64,
        input: AccountInput,
    ) -> Result<Account, BudgetError> {
        let kind = input.kind.unwrap_or_else(|| "chequing".into());
        if !ACCOUNT_KINDS.contains(&kind.as_str()) {
            return Err(BudgetError::Invalid("kind", "unknown account kind"));
        }
        let a = Account {
            id,
            owner_id: p.user_id.clone(),
            name: name(&input.name)?,
            kind,
            opening_balance: amount("opening_balance", input.opening_balance, false)?,
            archived: input.archived,
            created_at,
            updated_at: self.now,
        };
        let lower = a.name.to_lowercase();
        if b.accounts
            .iter()
            .any(|x| x.id != a.id && x.name.to_lowercase() == lower)
        {
            return Err(BudgetError::Invalid("name", "already used"));
        }
        Ok(a)
    }

    fn category(
        &self,
        p: &Principal,
        b: &Budget,
        id: String,
        created_at: i64,
        input: CategoryInput,
    ) -> Result<BudgetCategory, BudgetError> {
        if input.kind == Flow::Transfer {
            return Err(BudgetError::Invalid("kind", "expense or income"));
        }
        let monthly_budget = match input.monthly_budget {
            Some(_) if input.kind == Flow::Income => {
                return Err(BudgetError::Invalid(
                    "monthly_budget",
                    "only expense categories have a budget",
                ))
            }
            Some(v) => Some(amount("monthly_budget", v, true)?),
            None => None,
        };
        let c = BudgetCategory {
            id,
            owner_id: p.user_id.clone(),
            name: name(&input.name)?,
            kind: input.kind,
            color: color(input.color.as_deref())?,
            monthly_budget,
            created_at,
            updated_at: self.now,
        };
        let lower = c.name.to_lowercase();
        if b.categories
            .iter()
            .any(|x| x.id != c.id && x.kind == c.kind && x.name.to_lowercase() == lower)
        {
            return Err(BudgetError::Invalid("name", "already used"));
        }
        Ok(c)
    }

    fn transaction(
        &self,
        p: &Principal,
        b: &Budget,
        id: String,
        created_at: i64,
        template_id: Option<String>,
        input: TransactionInput,
    ) -> Result<Transaction, BudgetError> {
        check_flow(
            b,
            input.kind,
            &input.account_id,
            input.to_account_id.as_deref(),
            input.category_id.as_deref(),
        )?;
        Ok(Transaction {
            id,
            owner_id: p.user_id.clone(),
            kind: input.kind,
            account_id: input.account_id,
            to_account_id: input.to_account_id,
            category_id: input.category_id,
            amount: amount("amount", input.amount, true)?,
            date: date("date", &input.date)?,
            payee: payee(&input.payee)?,
            note: validate::text(&input.note, MAX_NOTE)
                .map_err(|w| BudgetError::Invalid("note", w))?,
            template_id,
            created_at,
            updated_at: self.now,
        })
    }

    fn template(
        &self,
        p: &Principal,
        b: &Budget,
        id: String,
        created_at: i64,
        input: TemplateInput,
    ) -> Result<Template, BudgetError> {
        check_flow(
            b,
            input.kind,
            &input.account_id,
            input.to_account_id.as_deref(),
            input.category_id.as_deref(),
        )?;
        if !CADENCES.contains(&input.cadence.as_str()) {
            return Err(BudgetError::Invalid("cadence", "unknown cadence"));
        }
        let next_date = date("next_date", &input.next_date)?;
        Ok(Template {
            id,
            owner_id: p.user_id.clone(),
            kind: input.kind,
            account_id: input.account_id,
            to_account_id: input.to_account_id,
            category_id: input.category_id,
            amount: amount("amount", input.amount, true)?,
            payee: payee(&input.payee)?,
            cadence: input.cadence,
            anchor_day: day_of(&next_date),
            next_date,
            active: input.active,
            created_at,
            updated_at: self.now,
        })
    }

    pub async fn create_account(
        &self,
        p: &Principal,
        input: AccountInput,
    ) -> Result<Account, BudgetError> {
        let id = self.fresh_id(p, Entity::Account, input.id.clone()).await?;
        let b = self.refs(p).await?;
        if b.accounts.len() >= MAX_ACCOUNTS {
            return Err(BudgetError::Invalid(
                "accounts",
                "limit of 50 accounts reached",
            ));
        }
        let a = self.account(p, &b, id, self.now, input)?;
        self.repo
            .insert(&p.user_id, &Item::Account(a.clone()))
            .await
            .map_err(backend)?;
        Ok(a)
    }

    pub async fn replace_account(
        &self,
        p: &Principal,
        id: &str,
        input: AccountInput,
    ) -> Result<Account, BudgetError> {
        let Item::Account(old) = self.existing(p, Entity::Account, id).await? else {
            return Err(BudgetError::NotFound);
        };
        let b = self.refs(p).await?;
        let a = self.account(p, &b, old.id, old.created_at, input)?;
        self.save(p, Item::Account(a.clone())).await?;
        Ok(a)
    }

    pub async fn create_category(
        &self,
        p: &Principal,
        input: CategoryInput,
    ) -> Result<BudgetCategory, BudgetError> {
        let id = self.fresh_id(p, Entity::Category, input.id.clone()).await?;
        let b = self.refs(p).await?;
        if b.categories.len() >= MAX_CATEGORIES {
            return Err(BudgetError::Invalid(
                "categories",
                "limit of 200 categories reached",
            ));
        }
        let c = self.category(p, &b, id, self.now, input)?;
        self.repo
            .insert(&p.user_id, &Item::Category(c.clone()))
            .await
            .map_err(backend)?;
        Ok(c)
    }

    pub async fn replace_category(
        &self,
        p: &Principal,
        id: &str,
        input: CategoryInput,
    ) -> Result<BudgetCategory, BudgetError> {
        let Item::Category(old) = self.existing(p, Entity::Category, id).await? else {
            return Err(BudgetError::NotFound);
        };
        if old.kind != input.kind {
            return Err(BudgetError::Invalid(
                "kind",
                "a category can't change direction",
            ));
        }
        let b = self.refs(p).await?;
        let c = self.category(p, &b, old.id, old.created_at, input)?;
        self.save(p, Item::Category(c.clone())).await?;
        Ok(c)
    }

    pub async fn create_transaction(
        &self,
        p: &Principal,
        input: TransactionInput,
    ) -> Result<Transaction, BudgetError> {
        let id = self
            .fresh_id(p, Entity::Transaction, input.id.clone())
            .await?;
        if self
            .repo
            .transaction_count(&p.user_id)
            .await
            .map_err(backend)?
            >= MAX_TRANSACTIONS
        {
            return Err(BudgetError::Invalid(
                "transactions",
                "limit of 10000 transactions reached",
            ));
        }
        let b = self.refs(p).await?;
        let t = self.transaction(p, &b, id, self.now, None, input)?;
        self.repo
            .insert(&p.user_id, &Item::Transaction(t.clone()))
            .await
            .map_err(backend)?;
        Ok(t)
    }

    pub async fn replace_transaction(
        &self,
        p: &Principal,
        id: &str,
        input: TransactionInput,
    ) -> Result<Transaction, BudgetError> {
        let Item::Transaction(old) = self.existing(p, Entity::Transaction, id).await? else {
            return Err(BudgetError::NotFound);
        };
        let b = self.refs(p).await?;
        let t = self.transaction(p, &b, old.id, old.created_at, old.template_id, input)?;
        self.save(p, Item::Transaction(t.clone())).await?;
        Ok(t)
    }

    pub async fn create_template(
        &self,
        p: &Principal,
        input: TemplateInput,
    ) -> Result<Template, BudgetError> {
        let id = self.fresh_id(p, Entity::Template, input.id.clone()).await?;
        let b = self.refs(p).await?;
        if b.templates.len() >= MAX_TEMPLATES {
            return Err(BudgetError::Invalid(
                "templates",
                "limit of 200 recurring items reached",
            ));
        }
        let t = self.template(p, &b, id, self.now, input)?;
        self.repo
            .insert(&p.user_id, &Item::Template(t.clone()))
            .await
            .map_err(backend)?;
        Ok(t)
    }

    pub async fn replace_template(
        &self,
        p: &Principal,
        id: &str,
        input: TemplateInput,
    ) -> Result<Template, BudgetError> {
        let Item::Template(old) = self.existing(p, Entity::Template, id).await? else {
            return Err(BudgetError::NotFound);
        };
        let b = self.refs(p).await?;
        let t = self.template(p, &b, old.id, old.created_at, input)?;
        self.save(p, Item::Template(t.clone())).await?;
        Ok(t)
    }

    async fn save(&self, p: &Principal, item: Item) -> Result<(), BudgetError> {
        if self
            .repo
            .replace(&p.user_id, &item)
            .await
            .map_err(backend)?
        {
            Ok(())
        } else {
            Err(BudgetError::NotFound)
        }
    }

    /// Deletes a record. An account still used by transactions or recurring
    /// items is refused (archive it instead); a category's items keep going
    /// uncategorised.
    pub async fn delete(&self, p: &Principal, entity: Entity, id: &str) -> Result<(), BudgetError> {
        if !is_id(id) {
            return Err(BudgetError::NotFound);
        }
        if entity == Entity::Account
            && self
                .repo
                .account_in_use(&p.user_id, id)
                .await
                .map_err(backend)?
        {
            return Err(BudgetError::Invalid(
                "account",
                "has transactions or recurring items; archive it instead",
            ));
        }
        if self
            .repo
            .delete(&p.user_id, entity, id)
            .await
            .map_err(backend)?
        {
            Ok(())
        } else {
            Err(BudgetError::NotFound)
        }
    }

    /// Posts a recurring item's pending occurrence (`next_date`) as a
    /// transaction with the client's id, and moves the item to the next
    /// date. A replay answers `Exists`.
    pub async fn post(
        &self,
        p: &Principal,
        template_id: &str,
        txn_id: &str,
    ) -> Result<(Transaction, Template), BudgetError> {
        if !is_id(txn_id) {
            return Err(BudgetError::Invalid("id", "expected 32 hex characters"));
        }
        if self
            .repo
            .get(&p.user_id, Entity::Transaction, txn_id)
            .await
            .map_err(backend)?
            .is_some()
        {
            return Err(BudgetError::Exists);
        }
        let Item::Template(mut tpl) = self.existing(p, Entity::Template, template_id).await? else {
            return Err(BudgetError::NotFound);
        };
        if !tpl.active {
            return Err(BudgetError::Invalid("template", "paused"));
        }
        if self
            .repo
            .transaction_count(&p.user_id)
            .await
            .map_err(backend)?
            >= MAX_TRANSACTIONS
        {
            return Err(BudgetError::Invalid(
                "transactions",
                "limit of 10000 transactions reached",
            ));
        }
        let b = self.refs(p).await?;
        let input = TransactionInput {
            id: None,
            kind: tpl.kind,
            account_id: tpl.account_id.clone(),
            to_account_id: tpl.to_account_id.clone(),
            category_id: tpl.category_id.clone(),
            amount: tpl.amount,
            date: tpl.next_date.clone(),
            payee: tpl.payee.clone(),
            note: String::new(),
        };
        let txn = self.transaction(p, &b, txn_id.into(), self.now, Some(tpl.id.clone()), input)?;
        tpl.next_date = next_occurrence(&tpl.next_date, &tpl.cadence, tpl.anchor_day);
        tpl.updated_at = self.now;
        self.repo
            .post(&p.user_id, &txn, &tpl)
            .await
            .map_err(backend)?;
        Ok((txn, tpl))
    }

    /// Stores many transactions (CSV import). Every one must carry a client
    /// id and be valid, or nothing is stored; ids already present are
    /// skipped, so a repeated import is safe.
    pub async fn import(
        &self,
        p: &Principal,
        inputs: Vec<TransactionInput>,
    ) -> Result<Imported, BudgetError> {
        if inputs.is_empty() {
            return Ok(Imported {
                imported: 0,
                skipped: 0,
            });
        }
        if inputs.len() > MAX_IMPORT {
            return Err(BudgetError::Invalid(
                "transactions",
                "at most 1000 per import",
            ));
        }
        let b = self.refs(p).await?;
        let mut seen = HashSet::new();
        let mut txns = Vec::with_capacity(inputs.len());
        for input in inputs {
            let id = match input.id.clone() {
                Some(id) if is_id(&id) => id,
                _ => return Err(BudgetError::Invalid("id", "every imported row needs an id")),
            };
            if !seen.insert(id.clone()) {
                return Err(BudgetError::Invalid("id", "repeated in the import"));
            }
            txns.push(self.transaction(p, &b, id, self.now, None, input)?);
        }
        let count = self
            .repo
            .transaction_count(&p.user_id)
            .await
            .map_err(backend)?;
        if count as usize + txns.len() > MAX_TRANSACTIONS as usize {
            return Err(BudgetError::Invalid(
                "transactions",
                "limit of 10000 transactions reached",
            ));
        }
        let imported = self
            .repo
            .insert_many(&p.user_id, &txns)
            .await
            .map_err(backend)?;
        Ok(Imported {
            imported,
            skipped: txns.len() as u32 - imported,
        })
    }
}

#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct MemoryBudget {
        pub currency: RefCell<Vec<(String, String)>>,
        pub items: RefCell<Vec<(String, Item)>>,
    }

    fn entity(i: &Item) -> Entity {
        match i {
            Item::Account(_) => Entity::Account,
            Item::Category(_) => Entity::Category,
            Item::Transaction(_) => Entity::Transaction,
            Item::Template(_) => Entity::Template,
        }
    }

    impl BudgetRepository for MemoryBudget {
        async fn load(&self, owner_id: &str, transactions: bool) -> Result<Budget, String> {
            let mut b = Budget {
                currency: self
                    .currency
                    .borrow()
                    .iter()
                    .find(|(o, _)| o == owner_id)
                    .map_or(DEFAULT_CURRENCY.into(), |(_, c)| c.clone()),
                ..Default::default()
            };
            for (o, i) in self.items.borrow().iter() {
                if o != owner_id {
                    continue;
                }
                match i.clone() {
                    Item::Account(a) => b.accounts.push(a),
                    Item::Category(c) => b.categories.push(c),
                    Item::Transaction(t) if transactions => b.transactions.push(t),
                    Item::Transaction(_) => {}
                    Item::Template(t) => b.templates.push(t),
                }
            }
            Ok(b)
        }
        async fn set_currency(&self, owner_id: &str, currency: &str, _: i64) -> Result<(), String> {
            let mut all = self.currency.borrow_mut();
            all.retain(|(o, _)| o != owner_id);
            all.push((owner_id.into(), currency.into()));
            Ok(())
        }
        async fn get(&self, owner_id: &str, e: Entity, id: &str) -> Result<Option<Item>, String> {
            Ok(self
                .items
                .borrow()
                .iter()
                .find(|(o, i)| o == owner_id && entity(i) == e && i.id() == id)
                .map(|(_, i)| i.clone()))
        }
        async fn transaction_count(&self, owner_id: &str) -> Result<u32, String> {
            Ok(self
                .items
                .borrow()
                .iter()
                .filter(|(o, i)| o == owner_id && entity(i) == Entity::Transaction)
                .count() as u32)
        }
        async fn account_in_use(&self, owner_id: &str, id: &str) -> Result<bool, String> {
            Ok(self.items.borrow().iter().any(|(o, i)| {
                o == owner_id
                    && match i {
                        Item::Transaction(t) => {
                            t.account_id == id || t.to_account_id.as_deref() == Some(id)
                        }
                        Item::Template(t) => {
                            t.account_id == id || t.to_account_id.as_deref() == Some(id)
                        }
                        _ => false,
                    }
            }))
        }
        async fn insert(&self, owner_id: &str, item: &Item) -> Result<(), String> {
            self.items
                .borrow_mut()
                .push((owner_id.into(), item.clone()));
            Ok(())
        }
        async fn replace(&self, owner_id: &str, item: &Item) -> Result<bool, String> {
            let mut all = self.items.borrow_mut();
            match all
                .iter_mut()
                .find(|(o, i)| o == owner_id && entity(i) == entity(item) && i.id() == item.id())
            {
                Some((_, i)) => {
                    *i = item.clone();
                    Ok(true)
                }
                None => Ok(false),
            }
        }
        async fn delete(&self, owner_id: &str, e: Entity, id: &str) -> Result<bool, String> {
            let mut all = self.items.borrow_mut();
            let before = all.len();
            all.retain(|(o, i)| !(o == owner_id && entity(i) == e && i.id() == id));
            let removed = all.len() < before;
            if e == Entity::Category {
                for (o, i) in all.iter_mut() {
                    if o != owner_id {
                        continue;
                    }
                    match i {
                        Item::Transaction(t) if t.category_id.as_deref() == Some(id) => {
                            t.category_id = None
                        }
                        Item::Template(t) if t.category_id.as_deref() == Some(id) => {
                            t.category_id = None
                        }
                        _ => {}
                    }
                }
            }
            Ok(removed)
        }
        async fn post(
            &self,
            owner_id: &str,
            txn: &Transaction,
            template: &Template,
        ) -> Result<(), String> {
            self.insert(owner_id, &Item::Transaction(txn.clone()))
                .await?;
            self.replace(owner_id, &Item::Template(template.clone()))
                .await?;
            Ok(())
        }
        async fn insert_many(&self, owner_id: &str, txns: &[Transaction]) -> Result<u32, String> {
            let mut n = 0;
            for t in txns {
                if self
                    .get(owner_id, Entity::Transaction, &t.id)
                    .await?
                    .is_none()
                {
                    self.insert(owner_id, &Item::Transaction(t.clone())).await?;
                    n += 1;
                }
            }
            Ok(n)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memory::MemoryBudget;
    use super::*;
    use crate::entropy::testing::CountingEntropy;
    use crate::test_util::block_on;

    fn owner(id: &str) -> Principal {
        Principal {
            user_id: id.into(),
            method: crate::identity::AuthMethod::Session,
        }
    }

    fn acct(name: &str, opening: i64) -> AccountInput {
        AccountInput {
            id: None,
            name: name.into(),
            kind: None,
            opening_balance: opening,
            archived: false,
        }
    }

    fn txn(kind: Flow, account: &str, amount: i64) -> TransactionInput {
        TransactionInput {
            id: None,
            kind,
            account_id: account.into(),
            to_account_id: None,
            category_id: None,
            amount,
            date: "2026-11-03".into(),
            payee: "Grocer".into(),
            note: String::new(),
        }
    }

    fn id(n: u8) -> String {
        format!("{n:032x}")
    }

    #[test]
    fn currency_is_three_letters() {
        assert_eq!(currency(" cad ").unwrap(), "CAD");
        assert!(currency("CA").is_err());
        assert!(currency("C$D").is_err());
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        assert_eq!(block_on(s.load(&owner("u"))).unwrap().currency, "CAD");
        block_on(s.set_currency(&owner("u"), "eur")).unwrap();
        assert_eq!(block_on(s.load(&owner("u"))).unwrap().currency, "EUR");
        assert_eq!(block_on(s.load(&owner("v"))).unwrap().currency, "CAD");
    }

    #[test]
    fn transactions_validate_amounts_accounts_and_categories() {
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        let p = owner("u");
        let a = block_on(s.create_account(&p, acct("Chequing", 10_000))).unwrap();
        let b = block_on(s.create_account(&p, acct("Savings", 0))).unwrap();
        let food = block_on(s.create_category(
            &p,
            CategoryInput {
                id: None,
                name: "Food".into(),
                kind: Flow::Expense,
                color: None,
                monthly_budget: Some(40_000),
            },
        ))
        .unwrap();
        let pay = block_on(s.create_category(
            &p,
            CategoryInput {
                id: None,
                name: "Pay".into(),
                kind: Flow::Income,
                color: None,
                monthly_budget: None,
            },
        ))
        .unwrap();

        assert!(matches!(
            block_on(s.create_transaction(&p, txn(Flow::Expense, &a.id, 0))),
            Err(BudgetError::Invalid("amount", _))
        ));
        assert!(matches!(
            block_on(s.create_transaction(&p, txn(Flow::Expense, &a.id, MAX_AMOUNT + 1))),
            Err(BudgetError::Invalid("amount", _))
        ));
        assert!(matches!(
            block_on(s.create_transaction(&p, txn(Flow::Expense, &id(9), 100))),
            Err(BudgetError::Invalid("account_id", _))
        ));
        let wrong_way = TransactionInput {
            category_id: Some(pay.id.clone()),
            ..txn(Flow::Expense, &a.id, 100)
        };
        assert!(matches!(
            block_on(s.create_transaction(&p, wrong_way)),
            Err(BudgetError::Invalid("category_id", _))
        ));
        let ok = TransactionInput {
            category_id: Some(food.id.clone()),
            ..txn(Flow::Expense, &a.id, 1_234)
        };
        let t = block_on(s.create_transaction(&p, ok)).unwrap();
        assert_eq!((t.amount, t.payee.as_str()), (1_234, "Grocer"));

        // Transfers need a different receiving account and no category.
        let to_self = TransactionInput {
            to_account_id: Some(a.id.clone()),
            ..txn(Flow::Transfer, &a.id, 500)
        };
        assert!(matches!(
            block_on(s.create_transaction(&p, to_self)),
            Err(BudgetError::Invalid("to_account_id", _))
        ));
        assert!(matches!(
            block_on(s.create_transaction(&p, txn(Flow::Transfer, &a.id, 500))),
            Err(BudgetError::Invalid("to_account_id", _))
        ));
        let transfer = TransactionInput {
            to_account_id: Some(b.id.clone()),
            ..txn(Flow::Transfer, &a.id, 500)
        };
        assert!(block_on(s.create_transaction(&p, transfer)).is_ok());

        // Another owner can't use these accounts.
        assert!(matches!(
            block_on(s.create_transaction(&owner("v"), txn(Flow::Expense, &a.id, 100))),
            Err(BudgetError::Invalid("account_id", _))
        ));
    }

    #[test]
    fn replayed_create_is_exists_and_replace_keeps_created_at() {
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        let p = owner("u");
        let input = AccountInput {
            id: Some(id(1)),
            ..acct("Cash", 0)
        };
        block_on(s.create_account(&p, input.clone())).unwrap();
        assert_eq!(
            block_on(s.create_account(&p, input)),
            Err(BudgetError::Exists)
        );
        assert!(matches!(
            block_on(s.create_account(&p, acct("cash", 0))),
            Err(BudgetError::Invalid("name", "already used"))
        ));
        let s2 = BudgetService::new(&repo, &e, 5);
        let a = block_on(s2.replace_account(
            &p,
            &id(1),
            AccountInput {
                archived: true,
                ..acct("Wallet", -2_500)
            },
        ))
        .unwrap();
        assert_eq!(
            (a.created_at, a.updated_at, a.archived, a.opening_balance),
            (1, 5, true, -2_500)
        );
        assert_eq!(
            block_on(s2.replace_account(&owner("v"), &id(1), acct("X", 0))),
            Err(BudgetError::NotFound)
        );
    }

    #[test]
    fn deleting_a_used_account_is_refused_and_a_category_is_cleared() {
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        let p = owner("u");
        let a = block_on(s.create_account(&p, acct("Chequing", 0))).unwrap();
        let c = block_on(s.create_category(
            &p,
            CategoryInput {
                id: None,
                name: "Food".into(),
                kind: Flow::Expense,
                color: Some("#ABC".into()),
                monthly_budget: None,
            },
        ))
        .unwrap();
        assert_eq!(c.color, "#aabbcc");
        let t = block_on(s.create_transaction(
            &p,
            TransactionInput {
                category_id: Some(c.id.clone()),
                ..txn(Flow::Expense, &a.id, 100)
            },
        ))
        .unwrap();
        assert!(matches!(
            block_on(s.delete(&p, Entity::Account, &a.id)),
            Err(BudgetError::Invalid("account", _))
        ));
        block_on(s.delete(&p, Entity::Category, &c.id)).unwrap();
        let b = block_on(s.load(&p)).unwrap();
        assert_eq!(b.transactions[0].id, t.id);
        assert_eq!(b.transactions[0].category_id, None);
        block_on(s.delete(&p, Entity::Transaction, &t.id)).unwrap();
        block_on(s.delete(&p, Entity::Account, &a.id)).unwrap();
        assert_eq!(
            block_on(s.delete(&p, Entity::Account, &a.id)),
            Err(BudgetError::NotFound)
        );
    }

    #[test]
    fn occurrences_keep_their_day_of_month() {
        assert_eq!(next_occurrence("2026-01-31", "monthly", 31), "2026-02-28");
        assert_eq!(next_occurrence("2026-02-28", "monthly", 31), "2026-03-31");
        assert_eq!(next_occurrence("2028-01-31", "monthly", 31), "2028-02-29");
        assert_eq!(next_occurrence("2026-12-15", "monthly", 15), "2027-01-15");
        assert_eq!(next_occurrence("2026-11-03", "weekly", 3), "2026-11-10");
        assert_eq!(next_occurrence("2026-11-03", "biweekly", 3), "2026-11-17");
        assert_eq!(next_occurrence("2028-02-29", "yearly", 29), "2029-02-28");
    }

    #[test]
    fn posting_a_template_records_it_once_and_moves_on() {
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        let p = owner("u");
        let a = block_on(s.create_account(&p, acct("Chequing", 0))).unwrap();
        let rent = block_on(s.create_template(
            &p,
            TemplateInput {
                id: None,
                kind: Flow::Expense,
                account_id: a.id.clone(),
                to_account_id: None,
                category_id: None,
                amount: 150_000,
                payee: "Rent".into(),
                cadence: "monthly".into(),
                next_date: "2026-10-31".into(),
                active: true,
            },
        ))
        .unwrap();
        assert_eq!(rent.anchor_day, 31);
        let (t, tpl) = block_on(s.post(&p, &rent.id, &id(7))).unwrap();
        assert_eq!(
            (t.date.as_str(), t.amount, t.template_id.as_deref()),
            ("2026-10-31", 150_000, Some(rent.id.as_str()))
        );
        assert_eq!(tpl.next_date, "2026-11-30");
        assert_eq!(
            block_on(s.post(&p, &rent.id, &id(7))),
            Err(BudgetError::Exists)
        );
        let b = block_on(s.load(&p)).unwrap();
        assert_eq!(
            (b.transactions.len(), b.templates[0].next_date.as_str()),
            (1, "2026-11-30")
        );
        assert!(matches!(
            block_on(s.create_template(
                &p,
                TemplateInput {
                    cadence: "daily".into(),
                    ..TemplateInput {
                        id: None,
                        kind: Flow::Income,
                        account_id: a.id.clone(),
                        to_account_id: None,
                        category_id: None,
                        amount: 1,
                        payee: String::new(),
                        cadence: String::new(),
                        next_date: "2026-11-01".into(),
                        active: true,
                    }
                }
            )),
            Err(BudgetError::Invalid("cadence", _))
        ));
    }

    #[test]
    fn import_is_all_or_nothing_and_skips_known_ids() {
        let repo = MemoryBudget::default();
        let e = CountingEntropy::default();
        let s = BudgetService::new(&repo, &e, 1);
        let p = owner("u");
        let a = block_on(s.create_account(&p, acct("Chequing", 0))).unwrap();
        let row = |n: u8| TransactionInput {
            id: Some(id(n)),
            ..txn(Flow::Expense, &a.id, 100 * i64::from(n))
        };
        let bad = TransactionInput {
            amount: -1,
            ..row(3)
        };
        assert!(block_on(s.import(&p, vec![row(1), bad])).is_err());
        assert_eq!(block_on(s.load(&p)).unwrap().transactions.len(), 0);
        assert_eq!(
            block_on(s.import(&p, vec![row(1), row(2)])).unwrap(),
            Imported {
                imported: 2,
                skipped: 0
            }
        );
        assert_eq!(
            block_on(s.import(&p, vec![row(2), row(3)])).unwrap(),
            Imported {
                imported: 1,
                skipped: 1
            }
        );
        assert!(matches!(
            block_on(s.import(&p, vec![row(4), row(4)])),
            Err(BudgetError::Invalid("id", _))
        ));
        let no_id = TransactionInput { id: None, ..row(5) };
        assert!(block_on(s.import(&p, vec![no_id])).is_err());
    }
}
