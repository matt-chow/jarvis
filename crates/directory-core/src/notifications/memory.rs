//! In-memory notification repository for tests. The notice log is the one
//! the reminder repository's scheduler saves write to.

use super::{delivery, Delivery, Notice, NotificationRepository, PushTransport, Subscription};
use std::cell::RefCell;

pub struct MemoryNotifications<'a> {
    pub notices: &'a RefCell<Vec<Notice>>,
    pub subscriptions: RefCell<Vec<Subscription>>,
    pub deliveries: RefCell<Vec<Delivery>>,
}

impl<'a> MemoryNotifications<'a> {
    pub fn new(notices: &'a RefCell<Vec<Notice>>) -> Self {
        Self {
            notices,
            subscriptions: RefCell::default(),
            deliveries: RefCell::default(),
        }
    }
}

impl NotificationRepository for MemoryNotifications<'_> {
    async fn subscriptions(&self, owner_id: &str) -> Result<Vec<Subscription>, String> {
        Ok(self
            .subscriptions
            .borrow()
            .iter()
            .filter(|s| s.owner_id == owner_id)
            .cloned()
            .collect())
    }
    async fn subscription_by_endpoint(
        &self,
        endpoint: &str,
    ) -> Result<Option<Subscription>, String> {
        Ok(self
            .subscriptions
            .borrow()
            .iter()
            .find(|s| s.endpoint == endpoint)
            .cloned())
    }
    async fn put_subscription(&self, s: &Subscription) -> Result<(), String> {
        let mut all = self.subscriptions.borrow_mut();
        all.retain(|x| x.id != s.id);
        all.push(s.clone());
        Ok(())
    }
    async fn delete_subscription(&self, owner_id: &str, id: &str) -> Result<bool, String> {
        let mut all = self.subscriptions.borrow_mut();
        let before = all.len();
        all.retain(|s| !(s.owner_id == owner_id && s.id == id));
        let gone = all.len() < before;
        if gone {
            for d in self.deliveries.borrow_mut().iter_mut() {
                if d.subscription_id == id
                    && [delivery::QUEUED, delivery::RETRY].contains(&d.state.as_str())
                {
                    d.state = delivery::EXPIRED.into();
                    d.next_try = None;
                }
            }
        }
        Ok(gone)
    }
    async fn insert_notice(&self, n: &Notice) -> Result<bool, String> {
        let mut log = self.notices.borrow_mut();
        let dup = log.iter().any(|x| {
            x.owner_id == n.owner_id
                && x.reminder_id == n.reminder_id
                && x.occurrence == n.occurrence
                && x.slot == n.slot
        });
        if !dup {
            log.push(n.clone());
        }
        Ok(!dup)
    }
    async fn pending_notices(&self, limit: u32) -> Result<Vec<Notice>, String> {
        let mut out: Vec<Notice> = self
            .notices
            .borrow()
            .iter()
            .filter(|n| n.state == super::notice::PENDING)
            .cloned()
            .collect();
        out.sort_by_key(|n| n.created_at);
        out.truncate(limit as usize);
        Ok(out)
    }
    async fn dispatch(&self, n: &Notice, deliveries: &[Delivery]) -> Result<(), String> {
        for x in self.notices.borrow_mut().iter_mut() {
            if x.id == n.id && x.owner_id == n.owner_id {
                x.state = n.state.clone();
            }
        }
        let mut all = self.deliveries.borrow_mut();
        for d in deliveries {
            if !all
                .iter()
                .any(|x| x.notice_id == d.notice_id && x.subscription_id == d.subscription_id)
            {
                all.push(d.clone());
            }
        }
        Ok(())
    }
    async fn due_deliveries(
        &self,
        now: i64,
        limit: u32,
    ) -> Result<Vec<(Delivery, Subscription)>, String> {
        let subs = self.subscriptions.borrow();
        let mut out: Vec<(Delivery, Subscription)> = self
            .deliveries
            .borrow()
            .iter()
            .filter(|d| {
                [delivery::QUEUED, delivery::RETRY].contains(&d.state.as_str())
                    && d.next_try.is_some_and(|t| t <= now)
            })
            .filter_map(|d| {
                subs.iter()
                    .find(|s| s.id == d.subscription_id && s.owner_id == d.owner_id)
                    .map(|s| (d.clone(), s.clone()))
            })
            .collect();
        out.sort_by_key(|(d, _)| d.next_try);
        out.truncate(limit as usize);
        Ok(out)
    }
    async fn save_delivery(&self, d: &Delivery, s: Option<&Subscription>) -> Result<(), String> {
        for x in self.deliveries.borrow_mut().iter_mut() {
            if x.id == d.id && x.owner_id == d.owner_id {
                *x = d.clone();
            }
        }
        if let Some(s) = s {
            for x in self.subscriptions.borrow_mut().iter_mut() {
                if x.id == s.id && x.owner_id == s.owner_id {
                    *x = s.clone();
                }
            }
        }
        Ok(())
    }
    async fn deliveries_for(
        &self,
        owner_id: &str,
        subscription_id: &str,
        states: &[&str],
        since: i64,
    ) -> Result<Vec<(Delivery, Notice)>, String> {
        let log = self.notices.borrow();
        Ok(self
            .deliveries
            .borrow()
            .iter()
            .filter(|d| {
                d.owner_id == owner_id
                    && d.subscription_id == subscription_id
                    && states.contains(&d.state.as_str())
                    && d.updated_at >= since
            })
            .filter_map(|d| {
                log.iter()
                    .find(|n| n.id == d.notice_id && n.owner_id == owner_id)
                    .map(|n| (d.clone(), n.clone()))
            })
            .collect())
    }
    async fn delivery(&self, owner_id: &str, id: &str) -> Result<Option<Delivery>, String> {
        Ok(self
            .deliveries
            .borrow()
            .iter()
            .find(|d| d.owner_id == owner_id && d.id == id)
            .cloned())
    }
    async fn history(
        &self,
        owner_id: &str,
        limit: u32,
    ) -> Result<Vec<(Notice, Vec<Delivery>)>, String> {
        let mut log: Vec<Notice> = self
            .notices
            .borrow()
            .iter()
            .filter(|n| n.owner_id == owner_id)
            .cloned()
            .collect();
        log.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        log.truncate(limit as usize);
        let all = self.deliveries.borrow();
        Ok(log
            .into_iter()
            .map(|n| {
                let ds = all
                    .iter()
                    .filter(|d| d.notice_id == n.id)
                    .cloned()
                    .collect();
                (n, ds)
            })
            .collect())
    }
    async fn prune(&self, before: i64) -> Result<(), String> {
        let mut log = self.notices.borrow_mut();
        let old: Vec<String> = log
            .iter()
            .filter(|n| n.created_at < before)
            .map(|n| n.id.clone())
            .collect();
        log.retain(|n| n.created_at >= before);
        self.deliveries
            .borrow_mut()
            .retain(|d| !old.contains(&d.notice_id));
        Ok(())
    }
}

/// A push the fake transport saw: the endpoint and its headers.
pub type SentPush = (String, Vec<(&'static str, String)>);

/// Answers each push with the next scripted status (default 201), and
/// records what was sent.
#[derive(Default)]
pub struct FakePush {
    pub replies: RefCell<Vec<Result<u16, String>>>,
    pub sent: RefCell<Vec<SentPush>>,
}

impl PushTransport for FakePush {
    async fn send(
        &self,
        endpoint: &str,
        headers: &[(&'static str, String)],
    ) -> Result<u16, String> {
        self.sent
            .borrow_mut()
            .push((endpoint.to_owned(), headers.to_vec()));
        let mut r = self.replies.borrow_mut();
        if r.is_empty() {
            Ok(201)
        } else {
            r.remove(0)
        }
    }
}
