//! Object storage abstraction — driver `s3` (S3/MinIO/R2) atau `local` (filesystem dev).
//! Kunci selalu berformat `users/{user_id}/{transcript_id}/source` (docs/08 §2).

use std::fs as std_fs;
use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::aws::AmazonS3Builder;
use object_store::local::LocalFileSystem;
use object_store::path::Path;
use object_store::{ObjectStore, ObjectStoreExt, WriteMultipart};
use tokio::io::AsyncReadExt;

use crate::config::Config;

const CHUNK_SIZE: usize = 5 * 1024 * 1024; // part minimum S3 multipart

#[derive(Clone)]
pub struct Storage {
    inner: Arc<dyn ObjectStore>,
}

impl Storage {
    pub fn from_config(cfg: &Config) -> Result<Self> {
        match cfg.storage_driver.as_str() {
            "local" => {
                let root = std::path::PathBuf::from(&cfg.storage_local_root);
                std_fs::create_dir_all(&root).with_context(|| {
                    format!("membuat direktori storage local {}", root.display())
                })?;
                let store =
                    LocalFileSystem::new_with_prefix(&root).context("init storage local gagal")?;
                tracing::info!(root = %root.display(), "storage: local filesystem");
                Ok(Self {
                    inner: Arc::new(store),
                })
            }
            "s3" => {
                let endpoint = cfg
                    .s3_endpoint
                    .clone()
                    .expect("S3_ENDPOINT wajib untuk driver s3");
                let bucket = cfg
                    .s3_bucket
                    .clone()
                    .expect("S3_BUCKET wajib untuk driver s3");
                let allow_http = endpoint.starts_with("http://");
                let store = AmazonS3Builder::new()
                    .with_endpoint(endpoint.clone())
                    .with_bucket_name(bucket.clone())
                    .with_access_key_id(cfg.s3_access_key.clone().unwrap_or_default())
                    .with_secret_access_key(cfg.s3_secret_key.clone().unwrap_or_default())
                    .with_region(cfg.s3_region.clone().unwrap_or_else(|| "us-east-1".into()))
                    .with_allow_http(allow_http)
                    .build()
                    .context("gagal membangun S3 client")?;
                tracing::info!(endpoint = %endpoint, bucket = %bucket, "storage: s3-compatible");
                Ok(Self {
                    inner: Arc::new(store),
                })
            }
            other => panic!("STORAGE_DRIVER tidak dikenal: {other}"),
        }
    }

    /// Driver `local` untuk test (root sementara).
    pub fn local_for_test(root: &std::path::Path) -> Self {
        std_fs::create_dir_all(root).expect("buat root storage test");
        let store = LocalFileSystem::new_with_prefix(root).expect("init storage test");
        Self {
            inner: Arc::new(store),
        }
    }

    /// Simpan file (di disk lokal/temp) ke object storage via multipart.
    pub async fn put_file(&self, key: &str, mut file: tokio::fs::File) -> Result<()> {
        let path = Path::from(key);
        // put_multipart (object_store 0.14) sinkron mengembalikan handle upload.
        let upload = self.inner.put_multipart(&path).await?;
        let mut writer = WriteMultipart::new_with_chunk_size(upload, CHUNK_SIZE);

        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = file.read(&mut buf).await.context("membaca file sumber")?;
            if n == 0 {
                break;
            }
            writer.write(&buf[..n]);
        }
        writer
            .finish()
            .await
            .context("menyelesaikan upload multipart")?;
        Ok(())
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        self.inner.delete(&Path::from(key)).await?;
        Ok(())
    }
}
