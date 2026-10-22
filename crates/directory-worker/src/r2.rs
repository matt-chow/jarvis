//! Private R2 bucket access for file content. The bucket has no public access;
//! content is only reachable through authenticated Worker routes.

use worker::{Bucket, Data, HttpMetadata, Object};

pub struct R2Blobs {
    bucket: Bucket,
}

impl R2Blobs {
    pub fn new(bucket: Bucket) -> Self {
        Self { bucket }
    }

    /// Store content. A streamed body must have a known length (a request
    /// body with Content-Length).
    pub async fn put(&self, key: &str, body: Data, content_type: &str) -> Result<(), String> {
        self.bucket
            .put(key, body)
            .http_metadata(HttpMetadata {
                content_type: Some(content_type.into()),
                ..Default::default()
            })
            .execute()
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "put returned no object".to_string())?;
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<Option<Object>, String> {
        self.bucket
            .get(key)
            .execute()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        self.bucket.delete(key).await.map_err(|e| e.to_string())
    }
}
