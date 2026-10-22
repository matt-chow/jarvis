//! Fixed-window rate limiting for unauthenticated endpoints (sign-in, setup).

/// A limit of `max` hits per `window_ms` for one key (e.g. client IP).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub max: u32,
    pub window_ms: i64,
}

/// Sign-in and setup: generous for a person, tight for a script.
/// One attempt is two requests (options + verify).
pub const AUTH_POLICY: Policy = Policy {
    max: 30,
    window_ms: 10 * 60 * 1000,
};

/// Atomic counter storage.
pub trait RateLimitStore {
    /// Increment the counter for `(key, window_start)` and return the new count.
    /// May also purge windows older than `purge_before`.
    async fn hit(&self, key: &str, window_start: i64, purge_before: i64) -> Result<u32, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    /// Rejected; retry after this many seconds.
    Limited {
        retry_after_secs: i64,
    },
}

pub async fn check<S: RateLimitStore>(
    store: &S,
    policy: Policy,
    key: &str,
    now: i64,
) -> Result<Decision, String> {
    let window_start = now - now.rem_euclid(policy.window_ms);
    let count = store
        .hit(key, window_start, window_start - policy.window_ms)
        .await?;
    if count <= policy.max {
        Ok(Decision::Allowed)
    } else {
        let retry_ms = window_start + policy.window_ms - now;
        Ok(Decision::Limited {
            retry_after_secs: (retry_ms + 999) / 1000,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::block_on;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory(RefCell<HashMap<(String, i64), u32>>);

    impl RateLimitStore for Memory {
        async fn hit(
            &self,
            key: &str,
            window_start: i64,
            purge_before: i64,
        ) -> Result<u32, String> {
            let mut m = self.0.borrow_mut();
            m.retain(|(_, w), _| *w >= purge_before);
            let c = m.entry((key.to_owned(), window_start)).or_insert(0);
            *c += 1;
            Ok(*c)
        }
    }

    const P: Policy = Policy {
        max: 3,
        window_ms: 60_000,
    };

    #[test]
    fn allows_up_to_max_then_limits_until_next_window() {
        let s = Memory::default();
        let t = 1_000_000_040_000; // 20 s into a 60 s window
        for _ in 0..3 {
            assert_eq!(block_on(check(&s, P, "ip1", t)), Ok(Decision::Allowed));
        }
        assert_eq!(
            block_on(check(&s, P, "ip1", t)),
            Ok(Decision::Limited {
                retry_after_secs: 40
            })
        );
        // Other keys are independent; the next window resets.
        assert_eq!(block_on(check(&s, P, "ip2", t)), Ok(Decision::Allowed));
        assert_eq!(
            block_on(check(&s, P, "ip1", t + 40_000)),
            Ok(Decision::Allowed)
        );
    }

    #[test]
    fn purges_old_windows() {
        let s = Memory::default();
        block_on(check(&s, P, "a", 0)).unwrap();
        block_on(check(&s, P, "b", 10 * 60_000)).unwrap();
        assert_eq!(s.0.borrow().len(), 1);
    }
}
