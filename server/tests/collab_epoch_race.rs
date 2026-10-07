//! Collaboration epochs against a real PostgreSQL: a REST replacement of the
//! content opens a new epoch atomically with the content write, and a save from
//! the abandoned room can never slip in — whatever the interleaving.
//!
//! Needs a scratch database: `OFFICE_TEST_DATABASE_URL=postgres://…`. Each test
//! works in its own throw-away schema (dropped at the end) holding a minimal
//! `documents` table, so it never touches office's real tables. Without the
//! variable the tests report that they were skipped and pass.

use std::time::Duration;

use kubuno_office::errors::OfficeError;
use kubuno_office::services::collab_epoch::{self as ce, CollabEntity, EpochChange};
use sqlx::postgres::PgPoolOptions;
use sqlx::{AssertSqlSafe, Executor, PgPool};
use uuid::Uuid;

struct Scratch {
    pool: PgPool,
    admin: PgPool,
    schema: String,
}

impl Scratch {
    async fn drop_schema(self) {
        self.pool.close().await;
        let _ = ddl(format!("DROP SCHEMA IF EXISTS {} CASCADE", self.schema)).execute(&self.admin).await;
    }
}

/// DDL naming the test's own generated schema (a UUID, never outside input).
fn ddl(sql: String) -> sqlx::RawSql {
    sqlx::raw_sql(AssertSqlSafe(sql))
}

async fn scratch() -> Option<Scratch> {
    let Ok(url) = std::env::var("OFFICE_TEST_DATABASE_URL") else {
        eprintln!("OFFICE_TEST_DATABASE_URL not set: collab epoch database tests skipped");
        return None;
    };
    let admin = PgPoolOptions::new().max_connections(1).connect(&url).await.expect("connect");
    let schema = format!("collab_epoch_test_{}", Uuid::new_v4().simple());
    ddl(format!("CREATE SCHEMA {schema}")).execute(&admin).await.expect("schema");
    let search_path = format!("SET search_path TO {schema}");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |conn, _| {
            let sql = search_path.clone();
            Box::pin(async move {
                ddl(sql).execute(&mut *conn).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .expect("connect");
    pool.execute(
        "CREATE TABLE documents (id UUID PRIMARY KEY, body TEXT NOT NULL, collab_epoch BIGINT NOT NULL DEFAULT 0)",
    )
    .await
    .expect("table");
    Some(Scratch { pool, admin, schema })
}

async fn new_doc(pool: &PgPool, body: &str) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO documents (id, body) VALUES ($1, $2)")
        .bind(id)
        .bind(body)
        .execute(pool)
        .await
        .expect("insert");
    id
}

async fn body(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT body FROM documents WHERE id = $1").bind(id).fetch_one(pool).await.expect("body")
}

/// A content save the way the handlers do it: epoch rule, then the content write,
/// all in one transaction holding the row lock.
async fn save(pool: &PgPool, id: Uuid, client_epoch: Option<i64>, content: &str) -> Result<EpochChange, OfficeError> {
    let mut tx = pool.begin().await?;
    let change = ce::begin_content_write(&mut tx, CollabEntity::Document, id, client_epoch).await?;
    sqlx::query("UPDATE documents SET body = $2 WHERE id = $1").bind(id).bind(content).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(change)
}

#[tokio::test]
async fn a_rest_replacement_abandons_the_room_and_its_saves() {
    let Some(db) = scratch().await else { return };
    let id = new_doc(&db.pool, "v0").await;

    // The web, in the room of epoch 0, saves: the room is the reference.
    let web = save(&db.pool, id, Some(0), "web edit").await.expect("room save");
    assert!(!web.replaced());

    // The desktop app saves through REST (no epoch): new epoch.
    let desktop = save(&db.pool, id, None, "desktop edit").await.expect("replacement");
    assert_eq!((desktop.previous, desktop.current), (0, 1));
    assert_eq!(ce::room_name(CollabEntity::Document, id, 1), format!("office-document:{id}~1"));

    // The web session still in room 0 autosaves its old state: refused, and the
    // content stays the desktop's.
    let stale = save(&db.pool, id, Some(0), "web old state").await;
    assert!(matches!(stale, Err(OfficeError::CollabEpochStale { current: 1 })), "{stale:?}");
    assert_eq!(body(&db.pool, id).await, "desktop edit");

    // Once reloaded into room 1, the web saves again.
    save(&db.pool, id, Some(1), "web after reload").await.expect("new room save");
    assert_eq!(ce::current(&db.pool, CollabEntity::Document, id).await.expect("read"), Some(1));
    db.drop_schema().await;
}

/// The race the epoch exists for: the web's autosave arrives WHILE the REST
/// replacement is writing the content. It must wait for the row lock, then be
/// refused — never land after the replacement with the old state.
#[tokio::test]
async fn a_room_save_racing_a_replacement_waits_then_is_refused() {
    let Some(db) = scratch().await else { return };
    let id = new_doc(&db.pool, "v0").await;

    // Replacement in progress: epoch bumped, content written, not committed yet.
    let mut tx = db.pool.begin().await.expect("tx");
    let change = ce::begin_content_write(&mut tx, CollabEntity::Document, id, None).await.expect("lock");
    sqlx::query("UPDATE documents SET body = 'desktop' WHERE id = $1").bind(id).execute(&mut *tx).await.expect("write");

    let pool = db.pool.clone();
    let web = tokio::spawn(async move { save(&pool, id, Some(0), "web old state").await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!web.is_finished(), "the room save must wait for the replacement's lock");

    tx.commit().await.expect("commit");
    let outcome = web.await.expect("join");
    assert!(matches!(outcome, Err(OfficeError::CollabEpochStale { current: 1 })), "{outcome:?}");
    assert_eq!(change.current, 1);
    assert_eq!(body(&db.pool, id).await, "desktop");
    db.drop_schema().await;
}

/// The other order: a room save holds the lock, the replacement waits and wins
/// afterwards — the stored content ends as the replacement, under a new epoch.
#[tokio::test]
async fn a_replacement_racing_a_room_save_lands_after_it() {
    let Some(db) = scratch().await else { return };
    let id = new_doc(&db.pool, "v0").await;

    let mut tx = db.pool.begin().await.expect("tx");
    ce::begin_content_write(&mut tx, CollabEntity::Document, id, Some(0)).await.expect("room save");
    sqlx::query("UPDATE documents SET body = 'web' WHERE id = $1").bind(id).execute(&mut *tx).await.expect("write");

    let pool = db.pool.clone();
    let desktop = tokio::spawn(async move { save(&pool, id, None, "desktop").await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!desktop.is_finished(), "the replacement must wait for the room save's lock");
    tx.commit().await.expect("commit");

    let change = desktop.await.expect("join").expect("replacement");
    assert_eq!((change.previous, change.current), (0, 1));
    assert_eq!(body(&db.pool, id).await, "desktop");
    // …and the room that saved before it is abandoned now.
    assert!(matches!(save(&db.pool, id, Some(0), "late").await, Err(OfficeError::CollabEpochStale { .. })));
    db.drop_schema().await;
}

/// A replacement whose content write fails leaves the epoch alone: the room is
/// still the reference for the content that is actually stored.
#[tokio::test]
async fn a_failed_replacement_keeps_the_room() {
    let Some(db) = scratch().await else { return };
    let id = new_doc(&db.pool, "v0").await;

    {
        let mut tx = db.pool.begin().await.expect("tx");
        let change = ce::begin_content_write(&mut tx, CollabEntity::Document, id, None).await.expect("lock");
        assert!(change.replaced());
        // The content write fails here: the transaction is dropped, not committed.
    }
    assert_eq!(ce::current(&db.pool, CollabEntity::Document, id).await.expect("read"), Some(0));
    save(&db.pool, id, Some(0), "web").await.expect("the room is still current");
    db.drop_schema().await;
}

#[tokio::test]
async fn a_missing_entity_is_not_found() {
    let Some(db) = scratch().await else { return };
    let missing = save(&db.pool, Uuid::new_v4(), None, "x").await;
    assert!(matches!(missing, Err(OfficeError::NotFound(_))), "{missing:?}");
    db.drop_schema().await;
}
