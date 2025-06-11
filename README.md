# CDK-MINTD ReDB to SQLite Migration Tool

This tool is designed to migrate a [CDK-MINTD](https://github.com/cashubtc/cdk) database from ReDB to SQLite format. It specifically targets version 10 of CDK-MINTD and should only be used for upgrading from this version.

## Purpose

The migration tool converts both the main mint database and the authentication database (if present) from ReDB to SQLite format. This migration is necessary for upgrading to newer versions of CDK-MINTD that use SQLite as the database backend.

## Requirements

- CDK-MINTD version 10 installed
- Both the original ReDB database and target SQLite database locations must be accessible
- Rust toolchain installed

## Installation

```bash
git clone https://github.com/thesimplekid/cdk-convert-redb-to-sqlite
cd cdk-convert-redb-to-sqlite
cargo build --release
```

## Usage

By default, the tool will look for the database in the default CDK-MINTD location (`~/.cdk-mintd/`).

```bash
./target/release/cdk-convert-redb-to-sqlite
```

To specify a custom directory:

```bash
./target/release/cdk-convert-redb-to-sqlite --work-dir /path/to/database/directory
```

## Safety Features

- The tool checks if a SQLite database already exists and will not overwrite it
- The original ReDB database is not modified during the migration
- Detailed logging of the migration process is provided

## What Gets Migrated

The tool migrates the following data:

### Main Database
- Mint information
- Quote TTL settings
- Proofs and their states (spent/pending)
- Mint and melt quotes
- Blind signatures
- Keysets

### Auth Database (if present)
- Auth proofs
- Protected endpoints
- Auth keysets
- Auth blind signatures

## Important Notes

1. **Backup**: Always backup your database before running the migration
2. **Version Check**: Only use this tool if you are migrating from CDK-MINTD version 10
3. **Verification**: After migration, verify that all data has been correctly transferred before using the new database

## Troubleshooting

If you encounter any issues during migration, the tool provides detailed logging that can help identify the problem. Common issues might include:

- Permission denied: Ensure you have write access to the target directory
- Database already exists: Remove or rename any existing SQLite database files
- Missing source database: Verify the ReDB database exists in the specified location

## Contributing

If you find any issues or have suggestions for improvements, please open an issue or submit a pull request on GitHub.

## License

This project is licensed under the MIT License - see the LICENSE file for details.
