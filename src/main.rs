use std::path::PathBuf;

use anyhow::{Result, anyhow};
use cdk_common::database::{
    MintAuthDatabase, MintDatabase, MintKeysDatabase, MintProofsDatabase, MintQuotesDatabase,
    MintSignaturesDatabase,
};
use cdk_common::nuts::Id;
use cdk_common::{AuthProof, BlindSignature, PublicKey, State};
use cdk_redb::MintRedbDatabase;
use cdk_redb::mint::MintRedbAuthDatabase;
use cdk_sqlite::MintSqliteDatabase;
use cdk_sqlite::mint::MintSqliteAuthDatabase;
use clap::Parser;
use redb::{Database, ReadableTable, TableDefinition};
use tracing_subscriber::EnvFilter;

use crate::cli::CLIArgs;

mod cli;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let default_filter = "debug";

    let sqlx_filter = "sqlx=warn";
    let hyper_filter = "hyper=warn";
    let h2_filter = "h2=warn";
    let tower_http = "tower_http=warn";

    let env_filter = EnvFilter::new(format!(
        "{default_filter},{sqlx_filter},{hyper_filter},{h2_filter},{tower_http}"
    ));

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let args = CLIArgs::parse();

    let work_dir = if let Some(work_dir) = args.work_dir {
        tracing::info!("Using work dir from cmd arg");
        work_dir
    } else {
        work_dir()?
    };

    let redb_path = work_dir.join("cdk-mintd.redb");
    let sql_db_path = work_dir.join("cdk-mintd.sqlite");

    tracing::info!("Starting database migration...");
    tracing::info!("Source ReDB: {:?}", redb_path);
    tracing::info!("Target SQLite: {:?}", sql_db_path);

    // Check if SQLite database already exists
    if sql_db_path.exists() {
        return Err(anyhow!(
            "SQLite database already exists at {:?}. Will not overwrite existing database.",
            sql_db_path
        ));
    }

    let sqlite_db = MintSqliteDatabase::new(&sql_db_path).await?;
    let redb_db = MintRedbDatabase::new(&redb_path)?;

    migrate_mint_info(&redb_db, &sqlite_db).await?;
    migrate_quotes(&redb_db, &sqlite_db).await?;

    let keysets = redb_db.get_keyset_infos().await?;
    let mut keyset_ids = vec![];

    for keyset in keysets {
        keyset_ids.push(keyset.id);
        sqlite_db.add_keyset_info(keyset).await?;
    }

    migrate_proofs(keyset_ids, &redb_db, &sqlite_db).await?;
    migrate_blind_signatures(&redb_path, &sqlite_db).await?;

    tracing::info!("Migration completed successfully!");
    tracing::info!("ReDB database: {:?}", redb_path);
    tracing::info!("SQLite database: {:?}", sql_db_path);

    // Auth database migration
    let auth_redb_path = work_dir.join("cdk-mintd-auth.redb");
    if auth_redb_path.exists() {
        tracing::info!("Auth database detected migrating.");

        let auth_sql_db_path = work_dir.join("cdk-mintd-auth.sqlite");
        let sqlite_auth_db = MintSqliteAuthDatabase::new(&auth_sql_db_path).await?;
        sqlite_auth_db.migrate().await;

        migrate_auth_blind_signatures(&auth_redb_path, &sqlite_auth_db).await?;

        let auth_proofs = get_auth_proofs(&auth_redb_path)?;
        let ys: Vec<PublicKey> = auth_proofs
            .iter()
            .map(|a| a.y().expect("valid y"))
            .collect();

        let redb_auth_db = MintRedbAuthDatabase::new(&auth_redb_path)?;
        let states = redb_auth_db.get_proofs_states(&ys).await?;

        assert_eq!(auth_proofs.len(), states.len());

        for (proof, state) in auth_proofs.into_iter().zip(states) {
            if let Some(state) = state {
                sqlite_auth_db
                    .update_proof_state(&proof.y().expect("Valid y"), state)
                    .await?;
            }

            sqlite_auth_db.add_proof(proof).await?;
        }

        migrate_auth_keysets(&redb_auth_db, &sqlite_auth_db).await?;
        migrate_protected_endpoints(&redb_auth_db, &sqlite_auth_db).await?;
    }

    Ok(())
}

async fn migrate_mint_info(
    redb_db: &MintRedbDatabase,
    sqlite_db: &MintSqliteDatabase,
) -> Result<()> {
    tracing::info!("Migrating mint info...");
    let mint_info = redb_db.get_mint_info().await?;
    sqlite_db.set_mint_info(mint_info).await?;

    tracing::info!("Migrating quote TTL info...");
    let quote_ttl_info = redb_db.get_quote_ttl().await?;
    sqlite_db.set_quote_ttl(quote_ttl_info).await?;

    tracing::info!("Mint info migration complete");
    Ok(())
}

async fn migrate_proofs(
    keysets: Vec<Id>,
    redb_db: &MintRedbDatabase,
    sqlite_db: &MintSqliteDatabase,
) -> Result<()> {
    tracing::info!("Starting proofs migration for {} keysets...", keysets.len());

    for (i, keyset) in keysets.iter().enumerate() {
        tracing::info!("Migrating proofs for keyset {}/{}", i + 1, keysets.len());
        let (keyset_proofs, states) = redb_db.get_proofs_by_keyset_id(keyset).await?;

        assert_eq!(keyset_proofs.len(), states.len());
        tracing::debug!("Found {} proofs for keyset", keyset_proofs.len());

        sqlite_db.add_proofs(keyset_proofs.clone(), None).await?;

        let mut spent_ys = vec![];
        let mut pending_ys = vec![];

        for (proof, state) in keyset_proofs.iter().zip(states) {
            if let Some(state) = state {
                match state {
                    State::Spent => {
                        spent_ys.push(proof.y()?);
                    }
                    State::Pending => {
                        pending_ys.push(proof.y()?);
                    }
                    _ => (),
                }
            }
        }

        tracing::debug!(
            "Updating states - Spent: {}, Pending: {}",
            spent_ys.len(),
            pending_ys.len()
        );
        sqlite_db
            .update_proofs_states(&spent_ys, State::Spent)
            .await?;
        sqlite_db
            .update_proofs_states(&pending_ys, State::Pending)
            .await?;
    }

    tracing::info!("Proofs migration complete");
    Ok(())
}

async fn migrate_quotes(redb_db: &MintRedbDatabase, sqlite_db: &MintSqliteDatabase) -> Result<()> {
    tracing::info!("Starting quotes migration...");
    let melt_quotes = redb_db.get_melt_quotes().await?;
    tracing::info!("Found {} melt quotes to migrate", melt_quotes.len());

    for (i, melt_quote) in melt_quotes.iter().enumerate() {
        tracing::debug!("Processing melt quote {}/{}", i + 1, melt_quotes.len());
        if let Ok(Some((melt_request, payment_key))) =
            redb_db.get_melt_request(&melt_quote.id).await
        {
            sqlite_db
                .add_melt_request(melt_request, payment_key)
                .await
                .ok();
        }

        sqlite_db.add_melt_quote(melt_quote.clone()).await?;
    }

    let mint_quotes = redb_db.get_mint_quotes().await?;
    tracing::info!("Found {} mint quotes to migrate", mint_quotes.len());

    for (i, mint_quote) in mint_quotes.iter().enumerate() {
        tracing::debug!("Processing mint quote {}/{}", i + 1, mint_quotes.len());
        sqlite_db.add_mint_quote(mint_quote.clone()).await?;
    }

    tracing::info!("Quotes migration complete");
    Ok(())
}

fn get_blind_signatures(redb_path: &PathBuf) -> Result<(Vec<PublicKey>, Vec<BlindSignature>)> {
    tracing::info!("Starting blind signatures migration...");

    const BLINDED_SIGNATURES: TableDefinition<[u8; 33], &str> =
        TableDefinition::new("blinded_signatures");

    let db = Database::create(redb_path)?;

    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(BLINDED_SIGNATURES)?;

    let (messages, sigs): (Vec<_>, Vec<_>) = table
        .iter()?
        .flatten()
        .map(|(m, s)| {
            let sig = serde_json::from_str::<BlindSignature>(s.value()).expect("Valid sig");
            let message = PublicKey::from_slice(&m.value()).expect("Valid message");

            (message, sig)
        })
        .collect();

    tracing::info!("Found {} blind signatures to migrate", messages.len());

    Ok((messages, sigs))
}

async fn migrate_blind_signatures(
    redb_path: &PathBuf,
    sqlite_db: &MintSqliteDatabase,
) -> Result<()> {
    let (messages, sigs) = get_blind_signatures(redb_path)?;
    sqlite_db
        .add_blind_signatures(&messages, &sigs, None)
        .await?;

    tracing::info!("Blind signatures migration complete");
    Ok(())
}

async fn migrate_auth_blind_signatures(
    redb_path: &PathBuf,
    sqlite_db: &MintSqliteAuthDatabase,
) -> Result<()> {
    let (messages, sigs) = get_blind_signatures(redb_path)?;
    sqlite_db.add_blind_signatures(&messages, &sigs).await?;
    tracing::info!("Auth Blind signatures migration complete");
    Ok(())
}

fn get_auth_proofs(redb_path: &PathBuf) -> Result<Vec<AuthProof>> {
    const PROOFS_TABLE: TableDefinition<[u8; 33], &str> = TableDefinition::new("proofs");

    let db = Database::create(redb_path)?;

    let read_txn = db.begin_read()?;
    let table = read_txn.open_table(PROOFS_TABLE)?;

    let auth_proofs: Vec<AuthProof> = table
        .iter()?
        .flatten()
        .map(|(_m, s)| serde_json::from_str::<AuthProof>(s.value()).expect("Valid sig"))
        .collect();

    Ok(auth_proofs)
}

async fn migrate_auth_keysets(
    redb_db: &MintRedbAuthDatabase,
    sqlite_db: &MintSqliteAuthDatabase,
) -> Result<()> {
    let keysets = redb_db.get_keyset_infos().await?;

    for keyset in keysets {
        sqlite_db.add_keyset_info(keyset).await?;
    }
    Ok(())
}

async fn migrate_protected_endpoints(
    redb_db: &MintRedbAuthDatabase,
    sqlite_db: &MintSqliteAuthDatabase,
) -> Result<()> {
    let protected_endpoints = redb_db.get_auth_for_endpoints().await?;
    let protected_endpoints = protected_endpoints
        .into_iter()
        .filter_map(|(k, v)| v.map(|a| (k, a)))
        .collect();

    sqlite_db
        .add_protected_endpoints(protected_endpoints)
        .await?;
    Ok(())
}

fn work_dir() -> Result<PathBuf> {
    let home_dir = home::home_dir().ok_or(anyhow!("Unknown home dir"))?;
    let dir = home_dir.join(".cdk-mintd");

    std::fs::create_dir_all(&dir)?;

    Ok(dir)
}
