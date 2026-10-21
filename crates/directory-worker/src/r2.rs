//! R2-backed `ObjectStore`. The bucket has no public access; objects are only
//! reachable through authenticated Worker routes.

use directory_core::storage::{ObjectInfo, ObjectKey, ObjectStore, StorageError, StoredObject};
use worker::{js_sys, Bucket, HttpMetadata, Object};

pub struct R2ObjectStore {
    bucket: Bucket,
}

impl R2ObjectStore {
    pub fn new(bucket: Bucket) -> Self {
        Self { bucket }
    }
}

fn backend(e: worker::Error) -> StorageError {
    StorageError::Backend(e.to_string())
}

fn info(obj: &Object) -> ObjectInfo {
    let uploaded: js_sys::Date = obj.uploaded().into();
    ObjectInfo {
        key: obj.key(),
        size: obj.size(),
        content_type: obj.http_metadata().content_type,
        uploaded_at: Some(String::from(uploaded.to_iso_string())),
    }
}

impl ObjectStore for R2ObjectStore {
    async fn put(
        &self,
        key: &ObjectKey,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<ObjectInfo, StorageError> {
        let obj = self
            .bucket
            .put(key.as_str(), body)
            .http_metadata(HttpMetadata {
                content_type: Some(content_type.into()),
                ..Default::default()
            })
            .execute()
            .await
            .map_err(backend)?
            // `None` only happens when an `only_if` precondition fails; we set none.
            .ok_or_else(|| StorageError::Backend("put returned no object".into()))?;
        Ok(info(&obj))
    }

    async fn get(&self, key: &ObjectKey) -> Result<Option<StoredObject>, StorageError> {
        let Some(obj) = self
            .bucket
            .get(key.as_str())
            .execute()
            .await
            .map_err(backend)?
        else {
            return Ok(None);
        };
        let meta = info(&obj);
        let body = match obj.body() {
            Some(b) => b.bytes().await.map_err(backend)?,
            None => Vec::new(),
        };
        Ok(Some(StoredObject { info: meta, body }))
    }

    async fn delete(&self, key: &ObjectKey) -> Result<(), StorageError> {
        self.bucket.delete(key.as_str()).await.map_err(backend)
    }

    async fn list(
        &self,
        prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<ObjectInfo>, StorageError> {
        let mut req = self.bucket.list().limit(limit);
        if let Some(p) = prefix {
            req = req.prefix(p);
        }
        let objects = req.execute().await.map_err(backend)?;
        Ok(objects.objects().iter().map(info).collect())
    }
}
