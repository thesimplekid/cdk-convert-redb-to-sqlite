use std::path::PathBuf;

use anyhow::Result;
use cdk_common::database::{
    MintAuthDatabase, MintDatabase, MintKeysDatabase, MintProofsDatabase, MintQuotesDatabase,
};
use cdk_redb::MintRedbDatabase;
use cdk_redb::mint::MintRedbAuthDatabase;
use cdk_sqlite::MintSqliteDatabase;
use cdk_sqlite::mint::MintSqliteAuthDatabase;

pub async fn verify_migration(work_dir: PathBuf) -> Result<()> {
    let redb_path = work_dir.join("cdk-mintd.redb");
    let sql_db_path = work_dir.join("cdk-mintd.sqlite");

    println!("\n=== Starting Database Verification ===");
    println!("Comparing ReDB: {:?}", redb_path);
    println!("With SQLite: {:?}\n", sql_db_path);

    let sqlite_db = MintSqliteDatabase::new(&sql_db_path).await?;
    let redb_db = MintRedbDatabase::new(&redb_path)?;

    // Verify mint info
    println!("📋 Checking mint info...");
    let redb_mint_info = redb_db.get_mint_info().await?;
    let sqlite_mint_info = sqlite_db.get_mint_info().await?;
    assert_eq!(redb_mint_info, sqlite_mint_info, "Mint info mismatch");
    println!("✅ Mint info matches");

    // Verify quote TTL
    println!("📋 Checking quote TTL...");
    let redb_quote_ttl = redb_db.get_quote_ttl().await?;
    let sqlite_quote_ttl = sqlite_db.get_quote_ttl().await?;
    assert_eq!(redb_quote_ttl, sqlite_quote_ttl, "Quote TTL mismatch");
    println!("✅ Quote TTL matches");

    // Verify keysets
    println!("📋 Checking keysets...");
    let redb_keysets = redb_db.get_keyset_infos().await?;
    let sqlite_keysets = sqlite_db.get_keyset_infos().await?;
    assert_eq!(
        redb_keysets.len(),
        sqlite_keysets.len(),
        "Keyset count mismatch"
    );
    for keyset in &redb_keysets {
        assert!(
            sqlite_keysets.contains(keyset),
            "Missing keyset in SQLite DB"
        );
    }
    println!("✅ All {} keysets match", redb_keysets.len());

    // Verify proofs for each keyset
    println!("📋 Checking proofs for each keyset...");
    let mut total_proofs = 0;
    for keyset in &redb_keysets {
        let (redb_proofs, redb_states) = redb_db.get_proofs_by_keyset_id(&keyset.id).await?;
        let (sqlite_proofs, _sqlite_states) = sqlite_db.get_proofs_by_keyset_id(&keyset.id).await?;

        assert_eq!(
            redb_proofs.len(),
            sqlite_proofs.len(),
            "Proof count mismatch for keyset"
        );

        for (redb_proof, redb_state) in redb_proofs.iter().zip(redb_states.iter()) {
            let matching_sqlite_proof = sqlite_proofs.iter().find(|p| p == &redb_proof);
            assert!(
                matching_sqlite_proof.is_some(),
                "Missing proof in SQLite DB"
            );

            if let Some(redb_state) = redb_state {
                let y = redb_proof.y()?;
                let sqlite_state = sqlite_db.get_proofs_states(&[y]).await?;
                assert_eq!(
                    redb_state,
                    &sqlite_state.first().unwrap().unwrap(),
                    "Proof state mismatch"
                );
            }
        }
        total_proofs += redb_proofs.len();
    }
    println!("✅ All {} proofs match across all keysets", total_proofs);

    // Verify quotes
    println!("📋 Checking quotes...");
    let redb_mint_quotes = redb_db.get_mint_quotes().await?;
    let sqlite_mint_quotes = sqlite_db.get_mint_quotes().await?;
    assert_eq!(
        redb_mint_quotes.len(),
        sqlite_mint_quotes.len(),
        "Mint quote count mismatch"
    );
    for quote in &redb_mint_quotes {
        assert!(
            sqlite_mint_quotes.contains(quote),
            "Missing mint quote in SQLite DB"
        );
    }
    println!("✅ All {} mint quotes match", redb_mint_quotes.len());

    let redb_melt_quotes = redb_db.get_melt_quotes().await?;
    let sqlite_melt_quotes = sqlite_db.get_melt_quotes().await?;
    assert_eq!(
        redb_melt_quotes.len(),
        sqlite_melt_quotes.len(),
        "Melt quote count mismatch"
    );
    for quote in &redb_melt_quotes {
        assert!(
            sqlite_melt_quotes.contains(quote),
            "Missing melt quote in SQLite DB"
        );
    }
    println!("✅ All {} melt quotes match", redb_melt_quotes.len());

    // Verify auth database if it exists
    let auth_redb_path = work_dir.join("cdk-mintd-auth.redb");
    if auth_redb_path.exists() {
        println!("\n=== Verifying Auth Database ===");
        let auth_sql_db_path = work_dir.join("cdk-mintd-auth.sqlite");

        let redb_auth_db = MintRedbAuthDatabase::new(&auth_redb_path)?;
        let sqlite_auth_db = MintSqliteAuthDatabase::new(&auth_sql_db_path).await?;

        // Verify auth keysets
        println!("📋 Checking auth keysets...");
        let redb_auth_keysets = redb_auth_db.get_keyset_infos().await?;
        let sqlite_auth_keysets = sqlite_auth_db.get_keyset_infos().await?;
        assert_eq!(
            redb_auth_keysets.len(),
            sqlite_auth_keysets.len(),
            "Auth keyset count mismatch"
        );
        for keyset in &redb_auth_keysets {
            assert!(
                sqlite_auth_keysets.contains(keyset),
                "Missing auth keyset in SQLite DB"
            );
        }
        println!("✅ All {} auth keysets match", redb_auth_keysets.len());

        // Verify protected endpoints
        println!("📋 Checking protected endpoints...");
        let redb_protected_endpoints = redb_auth_db.get_auth_for_endpoints().await?;
        let sqlite_protected_endpoints = sqlite_auth_db.get_auth_for_endpoints().await?;
        assert_eq!(
            redb_protected_endpoints.len(),
            sqlite_protected_endpoints.len(),
            "Protected endpoints count mismatch"
        );
        for (endpoint, auth) in &redb_protected_endpoints {
            let sqlite_auth = sqlite_protected_endpoints.get(endpoint);
            assert_eq!(
                auth,
                sqlite_auth.unwrap(),
                "Protected endpoint auth mismatch"
            );
        }
        println!(
            "✅ All {} protected endpoints match",
            redb_protected_endpoints.len()
        );
    }

    println!("=== Summary ===");
    println!("✓ Mint Info");
    println!("✓ Quote TTL");
    println!("✓ {} Keysets", redb_keysets.len());
    println!("✓ {} Total Proofs", total_proofs);
    println!("✓ {} Mint Quotes", redb_mint_quotes.len());
    println!("✓ {} Melt Quotes", redb_melt_quotes.len());
    if auth_redb_path.exists() {
        println!("✓ Auth Database Verified");
    }
    println!("===============\n");

    Ok(())
}
