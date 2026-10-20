use pixoma_studio::Store;
use std::path::PathBuf;

#[tokio::test]
async fn media_ranges_return_actual_bytes_and_reject_out_of_bounds_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local-cache");
    std::fs::create_dir_all(&root)?;
    let directory = tempfile::Builder::new().prefix("media-").tempdir_in(root)?;
    let store = Store::open(&directory.path().join("studio.sqlite")).await?;
    let session = store.create_session("account-a", "create-session").await?;
    let asset = store
        .create_text_asset(
            "account-a",
            &session.id,
            "说明.txt",
            "0123456789",
            "create-asset",
        )
        .await?;
    let version = &asset.versions[0].id;
    let part = store
        .asset_response("account-a", &asset.id, version, "GET", Some("bytes=2-5"))
        .await?;
    assert_eq!(part.status, 206);
    assert_eq!(part.body, b"2345");
    assert_eq!(part.content_range.as_deref(), Some("bytes 2-5/10"));
    let head = store
        .asset_response("account-a", &asset.id, version, "HEAD", None)
        .await?;
    assert!(head.body.is_empty());
    assert_eq!(head.content_length, 10);
    assert!(
        store
            .asset_response("account-a", &asset.id, version, "GET", Some("bytes=20-30"))
            .await
            .is_err()
    );
    assert!(
        store
            .asset_response("account-b", &asset.id, version, "GET", None)
            .await
            .is_err()
    );
    store.close().await;
    Ok(())
}
