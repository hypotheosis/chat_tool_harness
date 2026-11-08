use rusqlite::{Connection, Error as SqlError, Result as SqlResult};

const VERSION: i64 = 6;

pub fn apply(connection: &mut Connection) -> SqlResult<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(0..=VERSION).contains(&version) {
        return Err(SqlError::InvalidQuery);
    }
    if version < 1 {
        migrate_v1(connection)?;
    }
    if version < 2 {
        migrate_v2(connection)?;
    }
    if version < 3 {
        migrate_v3(connection)?;
    }
    if version < 4 {
        migrate_v4(connection)?;
    }
    if version < 5 {
        migrate_v5(connection)?;
    }
    if version < 6 {
        migrate_v6(connection)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn migrate_v4_for_test(connection: &mut Connection) -> SqlResult<()> {
    migrate_v1(connection)?;
    migrate_v2(connection)?;
    migrate_v3(connection)?;
    migrate_v4(connection)
}

#[cfg(test)]
pub(crate) fn migrate_v1_for_test(connection: &mut Connection) -> SqlResult<()> {
    migrate_v1(connection)
}
#[cfg(test)]
pub(crate) fn migrate_v2_for_test(connection: &mut Connection) -> SqlResult<()> {
    migrate_v2(connection)
}

fn migrate_v6(connection: &mut Connection) -> SqlResult<()> {
    let tx = connection.transaction()?;
    // Deletion invalidations must outlive their conversation and retain the event cursor.
    tx.execute_batch("ALTER TABLE conversations ADD COLUMN title TEXT NOT NULL DEFAULT 'Conversation';
        CREATE TABLE history_events (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, payload TEXT NOT NULL);
        INSERT INTO history_events SELECT id,conversation_id,payload FROM events;
        DROP TABLE events;
        ALTER TABLE history_events RENAME TO events;
        PRAGMA user_version = 6;")?;
    tx.commit()
}

fn migrate_v5(connection: &mut Connection) -> SqlResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE runs ADD COLUMN context_json TEXT NOT NULL DEFAULT '{}';
         CREATE TABLE derived_context (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id), run_id TEXT NOT NULL UNIQUE REFERENCES runs(id), payload TEXT NOT NULL);
         PRAGMA user_version = 5;",
    )?;
    transaction.commit()
}

fn migrate_v4(connection: &mut Connection) -> SqlResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "ALTER TABLE runs ADD COLUMN input_message_id TEXT;
        ALTER TABLE runs ADD COLUMN inference_count INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE runs ADD COLUMN attempt_count INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE runs ADD COLUMN active_ms INTEGER NOT NULL DEFAULT 0;
        ALTER TABLE runs ADD COLUMN partial_text TEXT NOT NULL DEFAULT '';
        ALTER TABLE runs ADD COLUMN failure TEXT;
        ALTER TABLE attempts ADD COLUMN call_json TEXT;
        ALTER TABLE attempts ADD COLUMN result_json TEXT;
        PRAGMA user_version = 4;",
    )?;
    transaction.commit()
}

fn migrate_v3(connection: &mut Connection) -> SqlResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE observed_tool_definitions (
        profile_id TEXT NOT NULL REFERENCES profiles(id),
        server_key TEXT NOT NULL,
        tool_name TEXT NOT NULL,
        schema_json TEXT NOT NULL,
        schema_digest TEXT NOT NULL,
        destination_digest TEXT NOT NULL,
        definition_digest TEXT NOT NULL,
        PRIMARY KEY(profile_id, server_key, tool_name)
    ); PRAGMA user_version = 3;",
    )?;
    transaction.commit()
}

fn migrate_v2(connection: &mut Connection) -> SqlResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE tool_permission_rules (
        profile_id TEXT NOT NULL REFERENCES profiles(id),
        server_key TEXT NOT NULL,
        tool_name TEXT NOT NULL,
        rule TEXT NOT NULL,
        definition_digest TEXT NOT NULL,
        PRIMARY KEY(profile_id, server_key, tool_name)
    ); PRAGMA user_version = 2;",
    )?;
    transaction.commit()
}

fn migrate_v1(connection: &mut Connection) -> SqlResult<()> {
    let transaction = connection.transaction()?;
    transaction.execute_batch(
        "CREATE TABLE endpoints (id TEXT PRIMARY KEY, payload TEXT NOT NULL);
         CREATE TABLE servers (id TEXT PRIMARY KEY, payload TEXT NOT NULL);
         CREATE TABLE profiles (id TEXT PRIMARY KEY, payload TEXT NOT NULL, revision INTEGER NOT NULL);
         CREATE TABLE profile_servers (profile_id TEXT NOT NULL REFERENCES profiles(id), server_id TEXT NOT NULL REFERENCES servers(id), PRIMARY KEY(profile_id, server_id));
         CREATE TABLE permission_rules (id TEXT PRIMARY KEY, profile_id TEXT NOT NULL REFERENCES profiles(id), payload TEXT NOT NULL);
         CREATE TABLE conversations (id TEXT PRIMARY KEY, current_revision INTEGER NOT NULL);
         CREATE TABLE conversation_snapshots (conversation_id TEXT NOT NULL REFERENCES conversations(id), revision INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY(conversation_id, revision));
         CREATE TABLE messages (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id), snapshot_revision INTEGER NOT NULL, ordinal INTEGER NOT NULL, payload TEXT NOT NULL, FOREIGN KEY(conversation_id, snapshot_revision) REFERENCES conversation_snapshots(conversation_id, revision));
         CREATE TABLE runs (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id), snapshot_revision INTEGER NOT NULL, state TEXT NOT NULL, FOREIGN KEY(conversation_id, snapshot_revision) REFERENCES conversation_snapshots(conversation_id, revision));
         CREATE TABLE attempts (id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(id), state TEXT NOT NULL, approval_binding TEXT);
         CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL REFERENCES conversations(id), payload TEXT NOT NULL);
         CREATE TABLE attachments (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id), payload TEXT NOT NULL);
         CREATE TABLE pages (id INTEGER PRIMARY KEY AUTOINCREMENT, attachment_id TEXT NOT NULL REFERENCES attachments(id), payload TEXT NOT NULL);
         CREATE TABLE artifacts (id TEXT PRIMARY KEY, conversation_id TEXT NOT NULL REFERENCES conversations(id), payload TEXT NOT NULL);
         CREATE TABLE disclosure_acknowledgments (id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL REFERENCES conversations(id), payload TEXT NOT NULL);
         PRAGMA user_version = 1;"
    )?;
    transaction.commit()
}
