//! Private object storage: key rules, limits, and the backend interface.
//!
//! `ObjectStore` is implemented by the Cloudflare R2 adapter in
//! `directory-worker` and by `MemoryStore` for tests.

use serde::Serialize;
use std::fmt;

pub const MAX_KEY_LEN: usize = 512;
/// Proof-of-concept upload cap; well under the Workers Free 100 MB body limit.
pub const MAX_OBJECT_BYTES: usize = 10 * 1024 * 1024;
pub const DEFAULT_LIST_LIMIT: u32 = 100;
pub const MAX_LIST_LIMIT: u32 = 1000;
const DEFAULT_CONTENT_TYPE: &str = "application/octet-stream";
const MAX_CONTENT_TYPE_LEN: usize = 127;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    InvalidKey(&'static str),
    TooLarge { max: usize },
    Backend(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey(why) => write!(f, "invalid key: {why}"),
            Self::TooLarge { max } => write!(f, "object exceeds {max} bytes"),
            Self::Backend(_) => write!(f, "storage backend error"),
        }
    }
}

/// A validated object key: `segment(/segment)*` using `[A-Za-z0-9._-]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectKey(String);

impl ObjectKey {
    pub fn parse(raw: &str) -> Result<Self, StorageError> {
        if raw.is_empty() {
            return Err(StorageError::InvalidKey("empty"));
        }
        if raw.len() > MAX_KEY_LEN {
            return Err(StorageError::InvalidKey("too long"));
        }
        for segment in raw.split('/') {
            validate_segment(segment)?;
        }
        Ok(Self(raw.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_segment(segment: &str) -> Result<(), StorageError> {
    if segment.is_empty() {
        return Err(StorageError::InvalidKey("empty path segment"));
    }
    if segment == "." || segment == ".." {
        return Err(StorageError::InvalidKey("relative path segment"));
    }
    if !segment
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(StorageError::InvalidKey("disallowed character"));
    }
    Ok(())
}

/// Validate a list prefix: a key, optionally ending in `/`.
fn validate_prefix(prefix: &str) -> Result<(), StorageError> {
    let trimmed = prefix.strip_suffix('/').unwrap_or(prefix);
    ObjectKey::parse(trimmed).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectInfo {
    pub key: String,
    pub size: u64,
    pub content_type: Option<String>,
    /// RFC 3339 / ISO 8601 upload time, when the backend reports one.
    pub uploaded_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredObject {
    pub info: ObjectInfo,
    pub body: Vec<u8>,
}

/// Backend-agnostic object storage. Implementations must keep objects private.
pub trait ObjectStore {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<ObjectInfo, StorageError>;
    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, StorageError>;
    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError>;
    async fn list(&self, prefix: Option<&str>, limit: u32)
        -> Result<Vec<ObjectInfo>, StorageError>;
}

impl<T: ObjectStore> ObjectStore for &T {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<ObjectInfo, StorageError> {
        (**self).put(key, body, content_type).await
    }
    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, StorageError> {
        (**self).get(key).await
    }
    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        (**self).delete(key).await
    }
    async fn list(
        &self,
        prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        (**self).list(prefix, limit).await
    }
}

/// Applies Directory's storage rules on top of any `ObjectStore`, scoped to one
/// owner: every physical key is `u/<owner_id>/<key>`, so one principal can never
/// address another's objects.
pub struct StorageService<S> {
    store: S,
    namespace: String,
}

impl<S: ObjectStore> StorageService<S> {
    pub fn new(store: S, owner_id: &str) -> Self {
        Self {
            store,
            namespace: format!("u/{owner_id}/"),
        }
    }

    fn physical(&self, raw_key: &str) -> Result<ObjectKey, StorageError> {
        let key = ObjectKey::parse(raw_key)?;
        Ok(ObjectKey(format!("{}{}", self.namespace, key.as_str())))
    }

    fn logical(&self, mut info: ObjectInfo) -> ObjectInfo {
        if let Some(k) = info.key.strip_prefix(&self.namespace) {
            info.key = k.to_owned();
        }
        info
    }

    pub async fn put(
        &self,
        raw_key: &str,
        body: Vec<u8>,
        content_type: Option<&str>,
    ) -> Result<ObjectInfo, StorageError> {
        let key = self.physical(raw_key)?;
        if body.len() > MAX_OBJECT_BYTES {
            return Err(StorageError::TooLarge {
                max: MAX_OBJECT_BYTES,
            });
        }
        let info = self
            .store
            .put(&key, body, &normalize_content_type(content_type))
            .await?;
        Ok(self.logical(info))
    }

    pub async fn get(&self, raw_key: &str) -> Result<Option<StoredObject>, StorageError> {
        let obj = self.store.get(&self.physical(raw_key)?).await?;
        Ok(obj.map(|o| StoredObject {
            info: self.logical(o.info),
            body: o.body,
        }))
    }

    pub async fn delete(&self, raw_key: &str) -> Result<(), StorageError> {
        self.store.delete(&self.physical(raw_key)?).await
    }

    pub async fn list(
        &self,
        prefix: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let prefix = prefix.filter(|p| !p.is_empty());
        if let Some(p) = prefix {
            validate_prefix(p)?;
        }
        let full_prefix = format!("{}{}", self.namespace, prefix.unwrap_or(""));
        let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
        let infos = self.store.list(Some(&full_prefix), limit).await?;
        Ok(infos.into_iter().map(|i| self.logical(i)).collect())
    }
}

/// Accept only simple `type/subtype` media types; anything else becomes a
/// generic binary type so stored content can never smuggle header values.
fn normalize_content_type(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return DEFAULT_CONTENT_TYPE.into();
    };
    let essence = raw
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let valid = essence.len() <= MAX_CONTENT_TYPE_LEN
        && essence.split_once('/').is_some_and(|(t, s)| {
            let ok = |p: &str| {
                !p.is_empty()
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"!#$&-^_.+".contains(&b))
            };
            ok(t) && ok(s)
        });
    if valid {
        essence
    } else {
        DEFAULT_CONTENT_TYPE.into()
    }
}

/// In-memory `ObjectStore` for tests and local experiments.
#[derive(Debug, Default)]
pub struct MemoryStore {
    objects: std::cell::RefCell<std::collections::BTreeMap<String, StoredObject>>,
}

impl ObjectStore for MemoryStore {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<ObjectInfo, StorageError> {
        let info = ObjectInfo {
            key: key.as_str().into(),
            size: body.len() as u64,
            content_type: Some(content_type.into()),
            uploaded_at: None,
        };
        let stored = StoredObject {
            info: info.clone(),
            body,
        };
        self.objects
            .borrow_mut()
            .insert(key.as_str().into(), stored);
        Ok(info)
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, StorageError> {
        Ok(self.objects.borrow().get(key.as_str()).cloned())
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        self.objects.borrow_mut().remove(key.as_str());
        Ok(())
    }

    async fn list(
        &self,
        prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        Ok(self
            .objects
            .borrow()
            .values()
            .filter(|o| prefix.is_none_or(|p| o.info.key.starts_with(p)))
            .take(limit as usize)
            .map(|o| o.info.clone())
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::block_on;

    #[test]
    fn accepts_valid_keys() {
        for k in [
            "a",
            "notes/2026/today.md",
            "photo_01-final.JPG",
            "a/.hidden",
        ] {
            assert!(ObjectKey::parse(k).is_ok(), "{k}");
        }
    }

    #[test]
    fn rejects_unsafe_keys() {
        let long = "a".repeat(MAX_KEY_LEN + 1);
        for k in [
            "",
            "/abs",
            "trailing/",
            "a//b",
            "../etc",
            "a/./b",
            "sp ace",
            "pct%2F",
            "ü",
            "a\\b",
            &long,
        ] {
            assert!(ObjectKey::parse(k).is_err(), "{k:?}");
        }
    }

    #[test]
    fn normalizes_content_types() {
        assert_eq!(normalize_content_type(None), DEFAULT_CONTENT_TYPE);
        assert_eq!(
            normalize_content_type(Some("Text/Plain; charset=utf-8")),
            "text/plain"
        );
        assert_eq!(
            normalize_content_type(Some("image/svg+xml")),
            "image/svg+xml"
        );
        assert_eq!(
            normalize_content_type(Some("text/html\r\nX-Evil: 1")),
            DEFAULT_CONTENT_TYPE
        );
        assert_eq!(
            normalize_content_type(Some("nonsense")),
            DEFAULT_CONTENT_TYPE
        );
    }

    #[test]
    fn round_trips_through_service() {
        let svc = StorageService::new(MemoryStore::default(), "owner1");
        let info = block_on(svc.put("docs/a.txt", b"hello".to_vec(), Some("text/plain"))).unwrap();
        assert_eq!(info.size, 5);

        let got = block_on(svc.get("docs/a.txt")).unwrap().unwrap();
        assert_eq!(got.body, b"hello");
        assert_eq!(got.info.content_type.as_deref(), Some("text/plain"));

        block_on(svc.put("other/b.bin", vec![0; 3], None)).unwrap();
        let listed = block_on(svc.list(Some("docs/"), None)).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            block_on(svc.list(None, Some(0))).unwrap().len(),
            1,
            "limit clamps to >= 1"
        );

        block_on(svc.delete("docs/a.txt")).unwrap();
        assert!(block_on(svc.get("docs/a.txt")).unwrap().is_none());
    }

    #[test]
    fn enforces_limits_and_validation() {
        let svc = StorageService::new(MemoryStore::default(), "owner1");
        let too_big = vec![0; MAX_OBJECT_BYTES + 1];
        assert_eq!(
            block_on(svc.put("big", too_big, None)),
            Err(StorageError::TooLarge {
                max: MAX_OBJECT_BYTES
            })
        );
        assert!(matches!(
            block_on(svc.get("../x")),
            Err(StorageError::InvalidKey(_))
        ));
        assert!(matches!(
            block_on(svc.list(Some("a//"), None)),
            Err(StorageError::InvalidKey(_))
        ));
    }

    #[test]
    fn owners_are_isolated() {
        let store = MemoryStore::default();
        let alice = StorageService::new(&store, "alice");
        let bob = StorageService::new(&store, "bob");
        let info = block_on(alice.put("notes/a.txt", b"hi".to_vec(), None)).unwrap();
        assert_eq!(info.key, "notes/a.txt", "callers see logical keys");
        assert!(store.objects.borrow().contains_key("u/alice/notes/a.txt"));
        assert!(block_on(bob.get("notes/a.txt")).unwrap().is_none());
        assert!(block_on(bob.list(None, None)).unwrap().is_empty());
        assert_eq!(
            block_on(alice.list(None, None)).unwrap()[0].key,
            "notes/a.txt"
        );
    }

    #[test]
    fn backend_errors_do_not_leak_details() {
        assert_eq!(
            StorageError::Backend("secret detail".into()).to_string(),
            "storage backend error"
        );
    }
}
