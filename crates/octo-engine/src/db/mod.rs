pub mod connection;
pub mod migrations;

pub use connection::Database;

use rusqlite::Connection;
use tracing::info;

const CURRENT_VERSION: u32 = 9;

pub fn get_migrations() -> Vec<migrations::Migration> {
    vec![
        migrations::migration_v1(),
        migrations::migration_v2(),
        migrations::migration_v3(),
        migrations::migration_v4(),
        migrations::migration_v5(),
        migrations::migration_v6(),
        migrations::migration_v7(),
        migrations::migration_v8(),
        migrations::migration_v9(),
    ]
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

    if version < CURRENT_VERSION {
        info!(
            from = version,
            to = CURRENT_VERSION,
            "Running database migration"
        );

        let migrations = get_migrations();
        for migration in migrations {
            if migration.version > version {
                migration.execute(conn)?;
                info!("Applied migration v{}", migration.version);
            }
        }

        conn.pragma_update(None, "user_version", CURRENT_VERSION)?;
        info!("Migration to v{CURRENT_VERSION} complete");
    }

    Ok(())
}
