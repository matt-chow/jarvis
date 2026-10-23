//! Private R2 bucket access for file content. The bucket has no public access;
//! content is only reachable through authenticated Worker routes.

use worker::{Bucket, Data, HttpMetadata, Object, Range};

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

    /// `length` bytes from `offset` (for HTTP Range requests).
    pub async fn get_range(
        &self,
        key: &str,
        offset: u64,
        length: u64,
    ) -> Result<Option<Object>, String> {
        self.bucket
            .get(key)
            .range(Range::OffsetWithLength { offset, length })
            .execute()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn delete(&self, key: &str) -> Result<(), String> {
        self.bucket.delete(key).await.map_err(|e| e.to_string())
    }
}

impl directory_core::files::BlobDeleter for R2Blobs {
    async fn delete(&self, key: &str) -> Result<(), String> {
        R2Blobs::delete(self, key).await
    }
}
