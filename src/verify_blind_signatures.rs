use std::path::PathBuf;

use anyhow::Result;
use cdk_common::database::{MintKeysDatabase, MintSignaturesDatabase};
use cdk_redb::MintRedbDatabase;
use cdk_sqlite::MintSqliteDatabase;

pub async fn verify_blind_signatures(work_dir: PathBuf) -> Result<()> {
    let redb_path = work_dir.join("cdk-mintd.redb");
    let sql_db_path = work_dir.join("cdk-mintd.sqlite");

    println!("\n=== Verifying Blind Signatures ===");

    let sqlite_db = MintSqliteDatabase::new(&sql_db_path).await?;
    let redb_db = MintRedbDatabase::new(&redb_path)?;

    // Get all keysets
    let keysets = redb_db.get_keyset_infos().await?;
    println!(
        "Checking blind signatures across {} keysets...",
        keysets.len()
    );

    let mut total_redb_amount = 0u64;
    let mut total_sqlite_amount = 0u64;
    let mut total_sigs = 0usize;

    // Check blind signatures for each keyset
    for keyset in keysets {
        println!("📋 Checking blind signatures for keyset: {}", keyset.id);

        // Get Redb blind signatures for this keyset
        let redb_sigs = redb_db.get_blind_signatures_for_keyset(&keyset.id).await?;
        let redb_amount_sum: u64 = redb_sigs.iter().map(|sig| u64::from(sig.amount)).sum();
        println!(
            "Found {} signatures in Redb with total amount {}",
            redb_sigs.len(),
            redb_amount_sum
        );

        // Get SQLite blind signatures for this keyset
        let sqlite_sigs = sqlite_db
            .get_blind_signatures_for_keyset(&keyset.id)
            .await?;
        let sqlite_amount_sum: u64 = sqlite_sigs.iter().map(|sig| u64::from(sig.amount)).sum();
        println!(
            "Found {} signatures in SQLite with total amount {}",
            sqlite_sigs.len(),
            sqlite_amount_sum
        );

        // Verify counts match for this keyset
        assert_eq!(
            redb_sigs.len(),
            sqlite_sigs.len(),
            "Blind signature count mismatch for keyset {}: Redb has {} but SQLite has {}",
            keyset.id,
            redb_sigs.len(),
            sqlite_sigs.len()
        );

        // Verify total amounts match for this keyset
        assert_eq!(
            redb_amount_sum, sqlite_amount_sum,
            "Total amount mismatch for keyset {}: Redb total is {} but SQLite total is {}",
            keyset.id, redb_amount_sum, sqlite_amount_sum
        );

        total_redb_amount += redb_amount_sum;
        total_sqlite_amount += sqlite_amount_sum;
        total_sigs += redb_sigs.len();

        println!("✅ All blind signatures match for keyset {}", keyset.id);
    }

    println!("\n✅ Blind signatures verification complete!");
    println!("Total blind signatures: {}", total_sigs);
    println!("Total amount: {} units", total_redb_amount);
    assert_eq!(
        total_redb_amount, total_sqlite_amount,
        "Total amounts don't match across all keysets"
    );
    println!("===============\n");

    Ok(())
}
