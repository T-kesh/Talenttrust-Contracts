# Escrow Storage Versioning and State Persistence

## Overview

This document describes the storage versioning system and migration strategy for the Talenttrust Escrow contract. The system ensures safe contract upgrades while preserving state integrity across versions.

## Design Principles

### 1. Immutability of Deployed Layouts

Once a storage layout version is deployed to the network, its key structure and value formats are **immutable**. This prevents accidental data corruption when reading historical state.

### 2. Explicit Versioning

All storage operations check the `LayoutVersion` metadata before reading or writing state. Unknown versions are rejected with `UnsupportedStorageVersion` error.

### 3. Namespace Isolation

Each storage version uses isolated namespaces to prevent key collisions:

- **V1 Namespaces:**
  - `meta_v1`: Version metadata and global counters
  - `escrow_v1`: Contract records
  - `rep_v1`: Reputation data

Future versions (V2, V3, etc.) will use separate namespaces (`escrow_v2`, `rep_v2`, etc.).

### 4. Migration Safety

Migrations are:
- **Idempotent**: Re-running the same migration is safe
- **Atomic**: All-or-nothing execution
- **Validated**: Layout integrity checked before and after
- **Non-destructive**: Original data preserved during migration

## Storage Layout V1

### Data Keys

```rust
#[contracttype]
enum DataKey {
    Meta(MetaKey),
    V1(V1Key),
}

#[contracttype]
enum MetaKey {
    LayoutVersion,      // u32: Current storage version
    NextContractId,     // u32: Auto-incrementing contract ID counter
}

#[contracttype]
enum V1Key {
    Contract(u32),           // EscrowRecord: Full contract state
    Reputation(Address),     // Reputation: Freelancer reputation aggregate
}
```

### Storage Version Enum

```rust
#[contracttype]
#[repr(u32)]
enum StorageVersion {
    V1 = 1,
}
```

### Contract Record Structure

```rust
#[contracttype]
pub struct EscrowRecord {
    pub client: Address,
    pub freelancer: Address,
    pub arbiter: Option<Address>,
    pub milestones: Vec<Milestone>,
    pub milestone_count: u32,
    pub total_amount: i128,
    pub funded_amount: i128,
    pub released_amount: i128,
    pub released_milestones: u32,
    pub status: ContractStatus,
    pub reputation_issued: bool,
}
```

### Reputation Structure

```rust
#[contracttype]
pub struct Reputation {
    pub total_rating: i128,
    pub ratings_count: u32,
}
```

## Migration System

### Initialization Behavior

When any storage operation is performed, the contract automatically:

1. Checks if `LayoutVersion` exists in storage
2. If missing: Initializes to `V1` (version 1)
3. If present and equals `V1`: Continues normally
4. If present but unsupported: Returns `UnsupportedStorageVersion` error

This ensures deterministic behavior on first deployment and protects against version mismatches.

### Migration Entrypoint

```rust
pub fn migrate_storage(env: Env, target_version: u32) -> Result<bool, EscrowError>
```

**Current Behavior (V1 only):**
- Target version `1`: Returns `true` (no-op)
- Any other target: Returns `UnsupportedMigrationTarget` error

**Future Behavior (when V2 is added):**

The migration will follow this pattern:

```rust
pub fn migrate_storage(env: Env, target_version: u32) -> Result<bool, EscrowError> {
    ensure_storage_layout(&env)?;
    
    let current_version = get_current_version(&env);
    
    // Idempotent: already at target
    if current_version == target_version {
        return Ok(true);
    }
    
    // Validate target is supported
    match target_version {
        1 => { /* V1 exists */ }
        2 => { /* V2 will be added */ }
        _ => return Err(EscrowError::UnsupportedMigrationTarget),
    }
    
    // Execute migration path
    match (current_version, target_version) {
        (1, 2) => migrate_v1_to_v2(&env)?,
        // Future: (2, 3) => migrate_v2_to_v3(&env)?,
        _ => return Err(EscrowError::UnsupportedMigrationTarget),
    }
    
    // Update version metadata only after successful migration
    set_version(&env, target_version);
    
    Ok(true)
}
```

### Migration Validation

Before and after migration, the system validates:

1. **Layout Version**: Current version is supported
2. **Key Integrity**: All expected keys are present
3. **Data Integrity**: Values can be deserialized correctly
4. **Counter Consistency**: NextContractId is valid

## Example Migration: V1 → V2 (Future)

When V2 is introduced, the migration will:

### Step 1: Read V1 Data

```rust
fn migrate_v1_to_v2(env: &Env) -> Result<(), EscrowError> {
    let storage = env.storage().persistent();
    
    // Read all V1 contracts
    let next_id = storage
        .get::<_, u32>(&DataKey::Meta(MetaKey::NextContractId))
        .unwrap_or(1);
    
    let mut v1_contracts = Vec::new(env);
    for contract_id in 1..next_id {
        if let Some(record) = storage.get::<_, EscrowRecord>(
            &DataKey::V1(V1Key::Contract(contract_id))
        ) {
            v1_contracts.push_back((contract_id, record));
        }
    }
    
    // ... continue migration
}
```

### Step 2: Transform Data

```rust
    // Transform V1 records to V2 format
    for (contract_id, v1_record) in v1_contracts.iter() {
        let v2_record = EscrowRecordV2 {
            // Copy existing fields
            client: v1_record.client.clone(),
            freelancer: v1_record.freelancer.clone(),
            // ... other fields
            
            // Add new V2 fields with defaults
            dispute_id: None,
            created_at: 0, // Historical contracts get epoch 0
        };
        
        // Write to V2 namespace
        storage.set(
            &DataKey::V2(V2Key::Contract(contract_id)),
            &v2_record
        );
    }
```

### Step 3: Validate Migration

```rust
    // Validate all contracts migrated successfully
    for (contract_id, _) in v1_contracts.iter() {
        let v2_record = storage
            .get::<_, EscrowRecordV2>(&DataKey::V2(V2Key::Contract(contract_id)))
            .ok_or(EscrowError::MigrationFailed)?;
        
        // Validate critical fields preserved
        // ...
    }
    
    Ok(())
}
```

### Step 4: Update Version

```rust
    // Only update version after successful migration
    storage.set(
        &DataKey::Meta(MetaKey::LayoutVersion),
        &(StorageVersion::V2 as u32)
    );
```

## Idempotency Guarantees

### Re-running Same Migration

```rust
// First run: performs migration
migrate_storage(env, 2); // V1 → V2

// Second run: detects already at target, returns immediately
migrate_storage(env, 2); // No-op, returns true
```

### Partial Migration Recovery

If a migration fails mid-execution:

1. Version metadata is **not** updated (still shows V1)
2. V1 data remains intact and readable
3. Partial V2 data may exist but is ignored (V1 is active)
4. Re-running migration will retry from V1 → V2

This ensures the contract never enters an inconsistent state.

## Testing Strategy

### Test Coverage

The storage migration system includes comprehensive tests:

1. **Version Initialization**
   - `test_storage_version_defaults_to_v1`: Verifies V1 initialization
   - `test_storage_version_initialized_on_first_access`: Validates lazy init

2. **Migration Idempotency**
   - `test_migrate_storage_to_current_version_is_noop`: V1 → V1 is safe
   - `test_migration_is_idempotent`: Multiple runs are safe

3. **Data Preservation**
   - `test_migration_noop_preserves_stored_contract_data`: Single contract
   - `test_migration_preserves_multiple_contracts`: Multiple contracts
   - `test_migration_preserves_funded_contract_state`: Complex state

4. **Error Handling**
   - `test_migrate_storage_rejects_unknown_target`: Invalid versions rejected
   - `test_migration_rejects_unsupported_versions`: Boundary testing

5. **Layout Validation**
   - `test_storage_layout_plan_namespaces`: Namespace stability
   - `test_migration_validates_layout_before_execution`: Pre-migration checks

6. **Post-Migration Operations**
   - `test_contract_operations_work_after_migration`: Normal ops continue

### Running Tests

```bash
# Run all storage tests
cargo test --package escrow storage

# Run with coverage
cargo llvm-cov test --package escrow storage

# Run specific test
cargo test --package escrow test_migration_is_idempotent
```

## Operational Procedures

### Deploying a New Version

1. **Development Phase**
   - Define new `StorageVersion` variant (e.g., `V2 = 2`)
   - Create new key variants (e.g., `V2Key`)
   - Implement migration function (`migrate_v1_to_v2`)
   - Add comprehensive tests

2. **Testing Phase**
   - Test migration on testnet with production-like data
   - Verify idempotency (run migration multiple times)
   - Validate data integrity after migration
   - Test rollback scenarios

3. **Deployment Phase**
   - Deploy new contract code (with V2 support)
   - Contract continues operating on V1 (backward compatible)
   - Call `migrate_storage(2)` to trigger migration
   - Monitor for errors

4. **Validation Phase**
   - Verify all contracts migrated successfully
   - Check storage version: `get_storage_version()` returns `2`
   - Validate contract operations work normally
   - Monitor gas costs and performance

### Rollback Strategy

If migration fails:

1. **Immediate**: Version metadata still shows V1, contract operates normally
2. **Investigation**: Analyze failure logs and state
3. **Fix**: Update migration logic in new contract version
4. **Retry**: Deploy fixed version and re-run migration

## Security Considerations

### Access Control

- Migration entrypoint is **public** (no admin required)
- This is safe because:
  - Migration is idempotent
  - Invalid targets are rejected
  - Data integrity is validated
  - Original data is preserved

### Denial of Service

- Migration gas costs are bounded by contract count
- Large migrations may require multiple transactions
- Future: Implement batched migration for large datasets

### Data Integrity

- All migrations validate data before updating version
- Checksums or merkle roots could be added for extra validation
- Critical fields are verified during migration

## Future Enhancements

### Planned Features

1. **Batched Migration**: Migrate contracts in chunks to reduce gas costs
2. **Migration Events**: Emit events for monitoring and auditing
3. **Rollback Support**: Explicit downgrade paths (V2 → V1)
4. **Migration Metrics**: Track migration progress and performance
5. **Automated Validation**: Post-migration integrity checks

### Version Roadmap

- **V1** (Current): Basic escrow with milestones and reputation
- **V2** (Planned): Add dispute resolution and timestamps
- **V3** (Future): Add multi-token support and advanced features

## API Reference

### Public Functions

```rust
/// Returns the current storage version (1 for V1)
pub fn get_storage_version(env: Env) -> Result<u32, EscrowError>

/// Returns the storage namespace plan
pub fn storage_layout_plan(env: Env) -> Result<StorageLayoutPlan, EscrowError>

/// Migrates storage to target version (idempotent)
pub fn migrate_storage(env: Env, target_version: u32) -> Result<bool, EscrowError>
```

### Error Codes

```rust
pub enum EscrowError {
    UnsupportedStorageVersion = 14,    // Current version not supported
    UnsupportedMigrationTarget = 15,   // Target version not supported
}
```

## References

- [Upgradeable Storage Planning](./upgradeable-storage.md)
- [Storage Tests](../../contracts/escrow/src/test/storage.rs)
- [Soroban Storage Documentation](https://soroban.stellar.org/docs/learn/persisting-data)

## Changelog

- **2026-04-22**: Enhanced migration system with comprehensive tests and documentation
- **2026-03-23**: Initial storage versioning implementation (V1)
