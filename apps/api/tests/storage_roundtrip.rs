//! Test cepat put/get storage local (diagnosis fase 3).

use object_store::ObjectStoreExt;
use object_store::local::LocalFileSystem;

#[tokio::test]
async fn put_get_local_roundtrip() {
    let root = std::env::temp_dir().join(format!("st-rt-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&root).unwrap();
    let store = LocalFileSystem::new_with_prefix(&root).unwrap();

    // PUT multipart (jalur yang dipakai Storage::put_file).
    let key = object_store::path::Path::from("users/u1/t1/source");
    let upload = store.put_multipart(&key).await.unwrap();
    let mut w = object_store::WriteMultipart::new_with_chunk_size(upload, 5 * 1024 * 1024);
    w.write(b"hello-audio-bytes");
    w.finish().await.unwrap();

    // GET (jalur Storage::get_file).
    let res = store.get(&key).await.unwrap();
    let bytes = res.bytes().await.unwrap();
    assert_eq!(bytes.as_ref(), b"hello-audio-bytes");

    let file = root.join("users/u1/t1/source");
    assert!(file.exists(), "file fisik harus ada: {}", file.display());
    std::fs::remove_dir_all(&root).ok();
}
