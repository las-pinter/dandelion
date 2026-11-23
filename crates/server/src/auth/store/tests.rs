use super::AuthStore;
use crate::storage::open_in_memory;

async fn store() -> AuthStore {
    AuthStore::new(open_in_memory().await.expect("in-memory database"))
}

#[test]
fn store_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<AuthStore>();
}

#[tokio::test]
async fn ensure_owner_creates_one_account() {
    let store = store().await;

    let first = store.ensure_owner(1).await.expect("create owner");
    let second = store.ensure_owner(2).await.expect("reuse owner");

    assert_eq!(first.id, second.id);
    assert_eq!(first.username, "owner");
    assert_eq!(first.display_name, "Owner");
    assert!(first.password_hash.is_none());
}

#[tokio::test]
async fn sessions_round_trip_and_slide() {
    let store = store().await;
    let owner = store.ensure_owner(1).await.expect("owner");

    store
        .insert_session("hash", &owner.id, 10, 100)
        .await
        .expect("insert");
    let session = store
        .session("hash")
        .await
        .expect("read")
        .expect("session exists");
    assert_eq!(session.user_id, owner.id);
    assert_eq!(session.expires_at, 100);

    store.touch_session("hash", 20, 200).await.expect("touch");
    let touched = store
        .session("hash")
        .await
        .expect("read")
        .expect("session exists");
    assert_eq!(touched.last_seen_at, 20);
    assert_eq!(touched.expires_at, 200);
}

#[tokio::test]
async fn deleting_sessions_scopes_to_the_user() {
    let store = store().await;
    let owner = store.ensure_owner(1).await.expect("owner");
    store
        .insert_session("first", &owner.id, 1, 100)
        .await
        .expect("insert");
    store
        .insert_session("second", &owner.id, 1, 100)
        .await
        .expect("insert");

    store.delete_sessions(&owner.id).await.expect("delete all");

    assert!(store.session("first").await.expect("read").is_none());
    assert!(store.session("second").await.expect("read").is_none());
}

#[tokio::test]
async fn setup_tokens_are_single_use() {
    let store = store().await;
    store
        .insert_setup_token("hash", 10, 100)
        .await
        .expect("insert");

    assert!(store.setup_token_valid("hash", 50).await.expect("valid"));
    assert!(
        store
            .consume_setup_token("hash", 50)
            .await
            .expect("consume")
    );
    assert!(!store.setup_token_valid("hash", 51).await.expect("valid"));
    assert!(
        !store
            .consume_setup_token("hash", 51)
            .await
            .expect("second consume")
    );
}

#[tokio::test]
async fn expired_setup_tokens_are_invalid() {
    let store = store().await;
    store
        .insert_setup_token("hash", 10, 100)
        .await
        .expect("insert");

    assert!(!store.setup_token_valid("hash", 100).await.expect("valid"));
    assert!(
        !store
            .consume_setup_token("hash", 100)
            .await
            .expect("consume")
    );
}

#[tokio::test]
async fn failures_are_counted_per_source() {
    let store = store().await;
    for now in [10, 20, 30] {
        store.record_failure("a", now).await.expect("record");
    }
    store.record_failure("b", 20).await.expect("record");

    assert_eq!(store.failures_since("a", 0).await.expect("count"), 3);
    assert_eq!(store.failures_since("a", 19).await.expect("count"), 2);
    assert_eq!(store.failures_since("a", 20).await.expect("count"), 1);
    assert_eq!(store.failures_since("b", 0).await.expect("count"), 1);
    assert_eq!(store.last_failure("a").await.expect("last"), Some(30));
}

#[tokio::test]
async fn wiping_removes_users_passkeys_and_sessions() {
    let store = store().await;
    let owner = store.ensure_owner(1).await.expect("owner");
    store
        .insert_passkey(&owner.id, b"credential", "{}", 1)
        .await
        .expect("passkey");
    store
        .insert_session("hash", &owner.id, 1, 100)
        .await
        .expect("session");
    store
        .insert_setup_token("setup", 1, 100)
        .await
        .expect("setup");

    store.wipe_auth().await.expect("wipe");

    assert!(store.owner().await.expect("read").is_none());
    assert_eq!(store.all_passkeys().await.expect("read").len(), 0);
    assert!(store.session("hash").await.expect("read").is_none());
    assert!(!store.setup_token_valid("setup", 2).await.expect("valid"));
}
