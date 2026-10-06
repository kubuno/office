//! Repairs of migration checksums recorded by installs that applied an earlier
//! revision of a migration file.
//!
//! sqlx refuses to start when an applied migration's recorded checksum differs
//! from the embedded file (`VersionMismatch`) and offers no way to accept a
//! previous revision. When an applied migration has to be edited anyway (a bug
//! that only shows on databases that have not applied it yet), its previous
//! checksum is listed in [`CHECKSUM_REPAIRS`]; before the migrator runs, every
//! row still carrying that exact previous checksum is moved to the current one.
//!
//! The repair is deliberately narrow: one UPDATE per listed version, matching
//! both the version and the exact previous checksum of a successful run. A row
//! with any other checksum (a locally modified file, an unknown revision) is
//! left alone, so sqlx still reports it.

use sqlx::migrate::{Migrator, MigrationType};
use sqlx::PgPool;

/// `(version, previous SHA-384 of the up file, hex)` of applied migrations
/// whose file was edited after release.
///
/// - 63: the release revision updated `office.slides.elements`, a column dropped
///   by migrations 11/17, so it failed on every fresh database. The fixed file
///   only runs that UPDATE when the column exists; the effect on a database that
///   applied the release revision is identical.
pub const CHECKSUM_REPAIRS: &[(i64, &str)] = &[
    (
        63,
        "bbd5cd3ffb5528fe6f791dcc87d726633562cf94269f626e5531347026e00348ac2732fc3bb6b8d2958ec8d645e11c5e",
    ),
    // The interim revision deployed on 2026-09-23 (UPDATE removed outright).
    (
        63,
        "725151f95071b020b203e26e457816f2edc671c6c71cf1b6ca29448c5886e07b02c37b0d1701277fc7ef821139ba7e7f",
    ),
];

/// Moves recorded checksums listed in [`CHECKSUM_REPAIRS`] to the checksum of
/// the embedded migration. Returns the number of rows repaired. Idempotent: a
/// second call finds no row with a previous checksum and changes nothing.
///
/// Must run before `Migrator::run`, after `office._sqlx_migrations` exists.
pub async fn repair_checksums(pool: &PgPool, migrator: &Migrator) -> Result<u64, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut repaired = 0;

    for (version, previous_hex) in CHECKSUM_REPAIRS {
        let Some(current) = migrator
            .iter()
            .find(|m| m.version == *version && !matches!(m.migration_type, MigrationType::ReversibleDown))
        else {
            tracing::warn!(version, "checksum repair listed for a migration that is not embedded");
            continue;
        };
        let previous = match hex::decode(previous_hex) {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::error!(version, error = %e, "invalid previous checksum in CHECKSUM_REPAIRS");
                continue;
            }
        };
        if previous.as_slice() == current.checksum.as_ref() {
            continue;
        }

        let result = sqlx::query(
            "UPDATE office._sqlx_migrations SET checksum = $1 \
             WHERE version = $2 AND checksum = $3 AND success",
        )
        .bind(current.checksum.as_ref())
        .bind(version)
        .bind(&previous)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(version, error = %e, "migration checksum repair failed");
            e
        })?;

        if result.rows_affected() > 0 {
            tracing::info!(
                version,
                "migration file was revised after being applied; recorded checksum updated to the current revision"
            );
            repaired += result.rows_affected();
        }
    }

    tx.commit().await?;
    Ok(repaired)
}

#[cfg(test)]
mod tests {
    use super::*;

    static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

    #[test]
    fn previous_checksums_are_sha384_and_differ_from_the_embedded_file() {
        for (version, previous_hex) in CHECKSUM_REPAIRS {
            let previous = hex::decode(previous_hex).expect("hex checksum");
            assert_eq!(previous.len(), 48, "SHA-384 for version {version}");
            let current = MIGRATOR
                .iter()
                .find(|m| m.version == *version && !matches!(m.migration_type, MigrationType::ReversibleDown))
                .expect("listed migration is embedded");
            assert_ne!(previous.as_slice(), current.checksum.as_ref(), "version {version}");
        }
    }
}
