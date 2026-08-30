use super::*;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn seed_message(pool: &SqlitePool, conversation_id: &str, branch: &str, user: &str, n: i64) {
    sqlx::query(
        "INSERT INTO ai_chat_messages
           (message_id, session_id, branch_id, user_id, role, content, message_type,
            created_at, conversation_id)
         VALUES (?, ?, ?, ?, 'user', ?, 'text', ?, ?)",
    )
    .bind(format!("m-{conversation_id}-{n}"))
    .bind(format!("s-{n}"))
    .bind(branch)
    .bind(user)
    .bind(format!("message {n}"))
    .bind(format!("2026-08-2{n}T10:00:00Z"))
    .bind(conversation_id)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn a_thread_is_named_after_the_first_thing_the_operator_said() {
    let pool = pool().await;
    ensure(&pool, "c1", "b1", "u1").await.unwrap();
    note_message(&pool, "c1", Some("What did we take yesterday?"))
        .await
        .unwrap();

    let listed = list(&pool, "b1", "u1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "What did we take yesterday?");
    assert_eq!(listed[0].message_count, 1);
}

/// A thread wanders. It keeps the name it was given rather than being renamed
/// by whatever was said most recently — and an operator's own title is never
/// overwritten by the next message.
#[tokio::test]
async fn later_messages_do_not_rename_the_thread() {
    let pool = pool().await;
    ensure(&pool, "c1", "b1", "u1").await.unwrap();
    note_message(&pool, "c1", Some("Check the milk price"))
        .await
        .unwrap();
    note_message(&pool, "c1", Some("Actually do the VAT return"))
        .await
        .unwrap();
    note_message(&pool, "c1", None).await.unwrap();

    let listed = list(&pool, "b1", "u1").await.unwrap();
    assert_eq!(listed[0].title, "Check the milk price");
    assert_eq!(listed[0].message_count, 3);

    rename(&pool, "c1", "b1", "u1", "VAT return, August")
        .await
        .unwrap();
    note_message(&pool, "c1", Some("and the stock count"))
        .await
        .unwrap();
    assert_eq!(
        list(&pool, "b1", "u1").await.unwrap()[0].title,
        "VAT return, August"
    );
}

/// Chat input arrives with newlines in it, and a title that wraps to four lines
/// turns a 260px history sidebar into a wall.
#[test]
fn a_title_is_one_line_and_cut_on_a_word() {
    assert_eq!(
        title_from("  check\n the   milk \n price "),
        "check the milk price"
    );

    let long = "please go through every product in the dairy category and tell me which ones have not sold";
    let title = title_from(long);
    assert!(title.chars().count() <= 80, "{}", title.chars().count());
    assert!(title.ends_with('…'));
    assert!(!title.contains("  "));
    // Cut between words, not mid-word.
    assert!(long.starts_with(title.trim_end_matches('…').trim()));

    // A single unbroken word still yields a name rather than an empty one.
    let unbroken = "a".repeat(200);
    assert_eq!(title_from(&unbroken).chars().count(), 80);
}

/// A conversation id arrives from the client. It is the whole record of what
/// somebody asked the AI to do with the shop, so ownership is checked on the
/// write rather than trusted.
#[tokio::test]
async fn a_thread_belonging_to_someone_else_is_refused() {
    let pool = pool().await;
    ensure(&pool, "c1", "b1", "u1").await.unwrap();

    assert!(matches!(
        ensure(&pool, "c1", "b1", "u2").await,
        Err(AppError::Permission(_))
    ));
    assert!(matches!(
        ensure(&pool, "c1", "b2", "u1").await,
        Err(AppError::Permission(_))
    ));
    // The rightful owner still gets through.
    assert!(ensure(&pool, "c1", "b1", "u1").await.is_ok());
}

#[tokio::test]
async fn reading_a_thread_is_scoped_to_its_owner_in_the_query() {
    let pool = pool().await;
    ensure(&pool, "c1", "b1", "u1").await.unwrap();
    seed_message(&pool, "c1", "b1", "u1", 1).await;

    assert_eq!(
        messages(&pool, "c1", "b1", "u1", 50).await.unwrap().len(),
        1
    );
    // Guessed from elsewhere: nothing, rather than someone else's chat.
    assert!(messages(&pool, "c1", "b1", "u2", 50)
        .await
        .unwrap()
        .is_empty());
    assert!(messages(&pool, "c1", "b2", "u1", 50)
        .await
        .unwrap()
        .is_empty());
}

/// The whole point of threads: yesterday's question does not arrive in today's
/// context. `load_history` used to take the last thirty messages regardless.
#[tokio::test]
async fn one_thread_never_returns_anothers_messages() {
    let pool = pool().await;
    for (id, n) in [("c1", 1), ("c2", 2)] {
        ensure(&pool, id, "b1", "u1").await.unwrap();
        seed_message(&pool, id, "b1", "u1", n).await;
        note_message(&pool, id, Some(&format!("thread {n}")))
            .await
            .unwrap();
    }

    let first = messages(&pool, "c1", "b1", "u1", 50).await.unwrap();
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].content, "message 1");
}

#[tokio::test]
async fn the_list_is_most_recent_first_and_hides_threads_never_used() {
    let pool = pool().await;
    ensure(&pool, "old", "b1", "u1").await.unwrap();
    note_message(&pool, "old", Some("older")).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    ensure(&pool, "new", "b1", "u1").await.unwrap();
    note_message(&pool, "new", Some("newer")).await.unwrap();
    // Opened and never spoken to: not history, and it would push real threads down.
    ensure(&pool, "untouched", "b1", "u1").await.unwrap();

    let listed = list(&pool, "b1", "u1").await.unwrap();
    assert_eq!(
        listed.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(),
        vec!["newer", "older"]
    );
}

/// A thread records instructions given to something that can change prices and
/// stock. Tidying the sidebar must not destroy that.
#[tokio::test]
async fn archiving_hides_the_thread_but_keeps_what_was_said() {
    let pool = pool().await;
    ensure(&pool, "c1", "b1", "u1").await.unwrap();
    seed_message(&pool, "c1", "b1", "u1", 1).await;
    note_message(&pool, "c1", Some("do the thing"))
        .await
        .unwrap();

    archive(&pool, "c1", "b1", "u1").await.unwrap();

    assert!(list(&pool, "b1", "u1").await.unwrap().is_empty());
    assert_eq!(
        messages(&pool, "c1", "b1", "u1", 50).await.unwrap().len(),
        1
    );
    // Archiving twice, or archiving someone else's, is a clean error.
    assert!(archive(&pool, "c1", "b1", "u1").await.is_err());
    assert!(archive(&pool, "c1", "b1", "u2").await.is_err());
}

#[tokio::test]
async fn reopening_lands_on_the_thread_last_spoken_to() {
    let pool = pool().await;
    assert!(most_recent(&pool, "b1", "u1").await.unwrap().is_none());

    ensure(&pool, "c1", "b1", "u1").await.unwrap();
    note_message(&pool, "c1", Some("first")).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    ensure(&pool, "c2", "b1", "u1").await.unwrap();
    note_message(&pool, "c2", Some("second")).await.unwrap();

    assert_eq!(
        most_recent(&pool, "b1", "u1").await.unwrap().as_deref(),
        Some("c2")
    );
    archive(&pool, "c2", "b1", "u1").await.unwrap();
    assert_eq!(
        most_recent(&pool, "b1", "u1").await.unwrap().as_deref(),
        Some("c1")
    );
}

/// Everything said before threads existed becomes one thread rather than
/// vanishing from a UI that only knows how to show threads.
#[tokio::test]
async fn the_migration_gathers_pre_existing_messages_into_one_thread() {
    let pool = pool().await;
    // Simulate rows written before 0053: no conversation_id.
    for n in 1..=3 {
        sqlx::query(
            "INSERT INTO ai_chat_messages
               (message_id, session_id, branch_id, user_id, role, content, message_type, created_at)
             VALUES (?, ?, 'b1', 'u1', 'user', 'legacy', 'text', ?)",
        )
        .bind(format!("legacy-{n}"))
        .bind(format!("s-{n}"))
        .bind(format!("2026-08-0{n}T10:00:00Z"))
        .execute(&pool)
        .await
        .unwrap();
    }
    // Re-run what the migration does, since migrations ran on an empty table.
    sqlx::query(
        "INSERT INTO ai_conversations
           (conversation_id, branch_id, user_id, title, message_count, last_message_at, created_at, updated_at)
         SELECT 'conv-legacy-' || branch_id || '-' || user_id, branch_id, user_id,
                'Earlier conversation', COUNT(*), MAX(created_at), MIN(created_at), MAX(created_at)
           FROM ai_chat_messages WHERE conversation_id IS NULL
          GROUP BY branch_id, user_id",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE ai_chat_messages
            SET conversation_id = 'conv-legacy-' || branch_id || '-' || user_id
          WHERE conversation_id IS NULL",
    )
    .execute(&pool)
    .await
    .unwrap();

    let listed = list(&pool, "b1", "u1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "Earlier conversation");
    assert_eq!(listed[0].message_count, 3);
    assert_eq!(
        messages(&pool, &listed[0].conversation_id, "b1", "u1", 50)
            .await
            .unwrap()
            .len(),
        3
    );
}
