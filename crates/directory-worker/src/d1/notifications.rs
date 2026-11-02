//! D1 implementation of `NotificationRepository` (migration 0027).

use super::{changes, err, int, opt_int, opt_text, stmt, text};
use directory_core::notifications::{
    delivery, Delivery, Notice, NotificationRepository, Subscription,
};
use serde::Deserialize;
use worker::wasm_bindgen::JsValue;
use worker::D1Database;

pub struct D1NotificationRepository {
    db: D1Database,
}

impl D1NotificationRepository {
    pub fn new(db: D1Database) -> Self {
        Self { db }
    }

    async fn subs(&self, filter: &str, args: &[JsValue]) -> Result<Vec<Subscription>, String> {
        let sql = format!("SELECT {SUB_COLUMNS} FROM push_subscriptions WHERE {filter}");
        Ok(stmt(&self.db, &sql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<SubRow>()
            .map_err(err)?
            .into_iter()
            .map(SubRow::into_sub)
            .collect())
    }

    async fn notices(&self, filter: &str, args: &[JsValue]) -> Result<Vec<Notice>, String> {
        let sql = format!("SELECT {NOTICE_COLUMNS} FROM notification_log WHERE {filter}");
        Ok(stmt(&self.db, &sql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<NoticeRow>()
            .map_err(err)?
            .into_iter()
            .map(NoticeRow::into_notice)
            .collect())
    }

    async fn deliveries(&self, filter: &str, args: &[JsValue]) -> Result<Vec<Delivery>, String> {
        let sql = format!("SELECT {DELIVERY_COLUMNS} FROM notification_deliveries WHERE {filter}");
        Ok(stmt(&self.db, &sql, args)?
            .all()
            .await
            .map_err(err)?
            .results::<DeliveryRow>()
            .map_err(err)?
            .into_iter()
            .map(DeliveryRow::into_delivery)
            .collect())
    }
}

const SUB_COLUMNS: &str =
    "id, owner_id, endpoint, label, created_at, updated_at, last_ok_at, last_error, failures";
const NOTICE_COLUMNS: &str =
    "id, owner_id, reminder_id, occurrence, slot, due_at, state, created_at";
const DELIVERY_COLUMNS: &str = "id, owner_id, notice_id, subscription_id, state, attempts, \
    next_try, status, created_at, updated_at";

#[derive(Deserialize)]
struct SubRow {
    id: String,
    owner_id: String,
    endpoint: String,
    label: String,
    created_at: f64,
    updated_at: f64,
    last_ok_at: Option<f64>,
    last_error: Option<String>,
    failures: f64,
}

impl SubRow {
    fn into_sub(self) -> Subscription {
        Subscription {
            id: self.id,
            owner_id: self.owner_id,
            endpoint: self.endpoint,
            label: self.label,
            created_at: self.created_at as i64,
            updated_at: self.updated_at as i64,
            last_ok_at: self.last_ok_at.map(|t| t as i64),
            last_error: self.last_error,
            failures: self.failures as u32,
        }
    }
}

#[derive(Deserialize)]
struct NoticeRow {
    id: String,
    owner_id: String,
    reminder_id: String,
    occurrence: String,
    slot: String,
    due_at: f64,
    state: String,
    created_at: f64,
}

impl NoticeRow {
    fn into_notice(self) -> Notice {
        Notice {
            id: self.id,
            owner_id: self.owner_id,
            reminder_id: self.reminder_id,
            occurrence: self.occurrence,
            slot: self.slot,
            due_at: self.due_at as i64,
            state: self.state,
            created_at: self.created_at as i64,
        }
    }
}

#[derive(Deserialize)]
struct DeliveryRow {
    id: String,
    owner_id: String,
    notice_id: String,
    subscription_id: String,
    state: String,
    attempts: f64,
    next_try: Option<f64>,
    status: Option<f64>,
    created_at: f64,
    updated_at: f64,
}

impl DeliveryRow {
    fn into_delivery(self) -> Delivery {
        Delivery {
            id: self.id,
            owner_id: self.owner_id,
            notice_id: self.notice_id,
            subscription_id: self.subscription_id,
            state: self.state,
            attempts: self.attempts as u32,
            next_try: self.next_try.map(|t| t as i64),
            status: self.status.map(|s| s as u16),
            created_at: self.created_at as i64,
            updated_at: self.updated_at as i64,
        }
    }
}

fn sub_args(s: &Subscription) -> Vec<JsValue> {
    vec![
        text(&s.id),
        text(&s.owner_id),
        text(&s.endpoint),
        text(&s.label),
        int(s.created_at),
        int(s.updated_at),
        opt_int(s.last_ok_at),
        opt_text(s.last_error.as_deref()),
        int(i64::from(s.failures)),
    ]
}

/// `?1, ?2, … ?n`
fn marks(n: usize) -> String {
    (1..=n)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

impl NotificationRepository for D1NotificationRepository {
    async fn subscriptions(&self, owner_id: &str) -> Result<Vec<Subscription>, String> {
        self.subs("owner_id = ?1", &[text(owner_id)]).await
    }

    async fn subscription_by_endpoint(
        &self,
        endpoint: &str,
    ) -> Result<Option<Subscription>, String> {
        Ok(self
            .subs("endpoint = ?1", &[text(endpoint)])
            .await?
            .into_iter()
            .next())
    }

    async fn put_subscription(&self, s: &Subscription) -> Result<(), String> {
        let sql = format!(
            "INSERT OR REPLACE INTO push_subscriptions ({SUB_COLUMNS}) VALUES ({})",
            marks(9)
        );
        stmt(&self.db, &sql, &sub_args(s))?
            .run()
            .await
            .map_err(err)?;
        Ok(())
    }

    async fn delete_subscription(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let args = [text(owner_id), text(id)];
        let batch = vec![
            stmt(
                &self.db,
                "DELETE FROM push_subscriptions WHERE owner_id = ?1 AND id = ?2",
                &args,
            )?,
            stmt(
                &self.db,
                &format!(
                    "UPDATE notification_deliveries SET state = '{}', next_try = NULL
                     WHERE owner_id = ?1 AND subscription_id = ?2 AND state IN ('{}', '{}')",
                    delivery::EXPIRED,
                    delivery::QUEUED,
                    delivery::RETRY
                ),
                &args,
            )?,
        ];
        let results = self.db.batch(batch).await.map_err(err)?;
        Ok(results.first().is_some_and(|r| changes(r) > 0))
    }

    async fn insert_notice(&self, n: &Notice) -> Result<bool, String> {
        let sql = format!(
            "INSERT OR IGNORE INTO notification_log ({NOTICE_COLUMNS}) VALUES ({})",
            marks(8)
        );
        let result = stmt(
            &self.db,
            &sql,
            &[
                text(&n.id),
                text(&n.owner_id),
                text(&n.reminder_id),
                text(&n.occurrence),
                text(&n.slot),
                int(n.due_at),
                text(&n.state),
                int(n.created_at),
            ],
        )?
        .run()
        .await
        .map_err(err)?;
        Ok(changes(&result) > 0)
    }

    async fn pending_notices(&self, limit: u32) -> Result<Vec<Notice>, String> {
        self.notices(
            "state = 'pending' ORDER BY created_at LIMIT ?1",
            &[int(i64::from(limit))],
        )
        .await
    }

    async fn dispatch(&self, n: &Notice, deliveries: &[Delivery]) -> Result<(), String> {
        let mut batch = vec![stmt(
            &self.db,
            "UPDATE notification_log SET state = ?3 WHERE owner_id = ?1 AND id = ?2",
            &[text(&n.owner_id), text(&n.id), text(&n.state)],
        )?];
        let sql = format!(
            "INSERT OR IGNORE INTO notification_deliveries ({DELIVERY_COLUMNS}) VALUES ({})",
            marks(10)
        );
        for d in deliveries {
            batch.push(stmt(&self.db, &sql, &delivery_args(d))?);
        }
        self.db.batch(batch).await.map_err(err)?;
        Ok(())
    }

    async fn due_deliveries(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<Vec<(Delivery, Subscription)>, String> {
        let ds = self
            .deliveries(
                "state IN ('queued', 'retry') AND next_try <= ?1 ORDER BY next_try LIMIT ?2",
                &[int(now), int(i64::from(limit))],
            )
            .await?;
        if ds.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<JsValue> = ds.iter().map(|d| text(&d.subscription_id)).collect();
        let subs = self
            .subs(&format!("id IN ({})", marks(ids.len())), &ids)
            .await?;
        // Paired only within the same owner.
        Ok(ds
            .into_iter()
            .filter_map(|d| {
                subs.iter()
                    .find(|s| s.id == d.subscription_id && s.owner_id == d.owner_id)
                    .cloned()
                    .map(|s| (d, s))
            })
            .collect())
    }

    async fn save_delivery(&self, d: &Delivery, s: Option<&Subscription>) -> Result<(), String> {
        let mut batch = vec![stmt(
            &self.db,
            "UPDATE notification_deliveries SET state = ?3, attempts = ?4, next_try = ?5,
             status = ?6, updated_at = ?7 WHERE owner_id = ?1 AND id = ?2",
            &[
                text(&d.owner_id),
                text(&d.id),
                text(&d.state),
                int(i64::from(d.attempts)),
                opt_int(d.next_try),
                opt_int(d.status.map(i64::from)),
                int(d.updated_at),
            ],
        )?];
        if let Some(s) = s {
            batch.push(stmt(
                &self.db,
                "UPDATE push_subscriptions SET last_ok_at = ?3, last_error = ?4, failures = ?5
                 WHERE owner_id = ?1 AND id = ?2",
                &[
                    text(&s.owner_id),
                    text(&s.id),
                    opt_int(s.last_ok_at),
                    opt_text(s.last_error.as_deref()),
                    int(i64::from(s.failures)),
                ],
            )?);
        }
        self.db.batch(batch).await.map_err(err)?;
        Ok(())
    }

    async fn deliveries_for(
        &self,
        owner_id: &str,
        subscription_id: &str,
        states: &[&str],
        since: i64,
    ) -> Result<Vec<(Delivery, Notice)>, String> {
        let mut args = vec![text(owner_id), text(subscription_id), int(since)];
        let first = args.len() + 1;
        args.extend(states.iter().map(|s| text(s)));
        let ins: Vec<String> = (first..args.len() + 1).map(|i| format!("?{i}")).collect();
        let ds = self
            .deliveries(
                &format!(
                    "owner_id = ?1 AND subscription_id = ?2 AND updated_at >= ?3
                     AND state IN ({}) ORDER BY created_at LIMIT 20",
                    ins.join(", ")
                ),
                &args,
            )
            .await?;
        if ds.is_empty() {
            return Ok(Vec::new());
        }
        let mut nargs = vec![text(owner_id)];
        nargs.extend(ds.iter().map(|d| text(&d.notice_id)));
        let ns = self
            .notices(
                &format!(
                    "owner_id = ?1 AND id IN ({})",
                    (2..nargs.len() + 1)
                        .map(|i| format!("?{i}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                &nargs,
            )
            .await?;
        Ok(ds
            .into_iter()
            .filter_map(|d| {
                ns.iter()
                    .find(|n| n.id == d.notice_id)
                    .cloned()
                    .map(|n| (d, n))
            })
            .collect())
    }

    async fn delivery(&self, owner_id: &str, id: &str) -> Result<Option<Delivery>, String> {
        Ok(self
            .deliveries("owner_id = ?1 AND id = ?2", &[text(owner_id), text(id)])
            .await?
            .into_iter()
            .next())
    }

    async fn history(
        &self,
        owner_id: &str,
        limit: u32,
    ) -> Result<Vec<(Notice, Vec<Delivery>)>, String> {
        let ns = self
            .notices(
                "owner_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2",
                &[text(owner_id), int(i64::from(limit))],
            )
            .await?;
        if ns.is_empty() {
            return Ok(Vec::new());
        }
        let ds = self
            .deliveries(
                "owner_id = ?1 AND notice_id IN (SELECT id FROM notification_log
                 WHERE owner_id = ?1 ORDER BY created_at DESC, rowid DESC LIMIT ?2)",
                &[text(owner_id), int(i64::from(limit))],
            )
            .await?;
        Ok(ns
            .into_iter()
            .map(|n| {
                let mine = ds.iter().filter(|d| d.notice_id == n.id).cloned().collect();
                (n, mine)
            })
            .collect())
    }

    async fn prune(&self, before: i64) -> Result<(), String> {
        let args = [int(before)];
        let batch = vec![
            stmt(
                &self.db,
                "DELETE FROM notification_deliveries WHERE notice_id IN
                 (SELECT id FROM notification_log WHERE created_at < ?1)",
                &args,
            )?,
            stmt(
                &self.db,
                "DELETE FROM notification_log WHERE created_at < ?1",
                &args,
            )?,
        ];
        self.db.batch(batch).await.map_err(err)?;
        Ok(())
    }
}

fn delivery_args(d: &Delivery) -> Vec<JsValue> {
    vec![
        text(&d.id),
        text(&d.owner_id),
        text(&d.notice_id),
        text(&d.subscription_id),
        text(&d.state),
        int(i64::from(d.attempts)),
        opt_int(d.next_try),
        opt_int(d.status.map(i64::from)),
        int(d.created_at),
        int(d.updated_at),
    ]
}
