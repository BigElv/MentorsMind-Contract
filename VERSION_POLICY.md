# Contract Version Management Policy

This document defines the versioning strategy and change management policy for MentorsMind smart contracts.

## Overview

Effective contract versioning is crucial for maintaining trust and enabling smooth upgrades. This policy ensures that all changes are properly tracked, communicated, and managed throughout the contract lifecycle.

## Version Numbering Scheme

We use **Semantic Versioning** (SemVer) with format: `MAJOR.MINOR.PATCH`

- **MAJOR** (X.0.0): Breaking changes that require migration
- **MINOR** (0.Y.0): New features, backward-compatible changes  
- **PATCH** (0.0.Z): Bug fixes, optimizations, fully compatible

### Examples
- `1.0.0` → `2.0.0`: Breaking change (major)
- `1.0.0` → `1.1.0`: New feature (minor)
- `1.0.0` → `1.0.1`: Bug fix (patch)

## Breaking Changes

Breaking changes require consumers to update their integration and may require data migration.

### What Constitutes a Breaking Change

#### Storage Changes
- **Modifying existing storage keys**: Changes to data structure layout
- **Removing storage keys**: Deleting persistent data
- **Changing data types**: Altering the type of stored values
- **Reordering storage layout**: Changing the sequence of storage operations

#### Function Interface Changes
- **Function signature modifications**: Changing parameter types, order, or count
- **Return type changes**: Altering what functions return
- **Removing public functions**: Deleting existing callable functions
- **Changing function behavior**: Modifying core logic in incompatible ways

#### Event Changes
- **Event structure modifications**: Changing event data format
- **Removing events**: Eliminating existing event emissions
- **Changing event topics**: Modifying event identifiers

#### Business Logic Changes
- **Fee calculation changes**: Modifying how fees are computed
- **Authorization changes**: Altering who can call functions
- **Timing changes**: Modifying delays, timeouts, or time-based logic
- **Token economics changes**: Affecting token transfers or balances

### Breaking Change Examples

```rust
// ❌ BREAKING: Changing storage key
const FEE_BPS: Symbol = symbol_short!("FEE"); // was FEE_BPS

// ❌ BREAKING: Changing function signature
pub fn create_escrow(env: Env, amount: i128) // removed mentor parameter

// ❌ BREAKING: Changing return type
pub fn get_balance(env: Env) -> String // was i128

// ❌ BREAKING: Changing event structure
env.events().publish(("created", id), (amount,)) // was (amount, mentor)
```

## Non-Breaking Changes

Non-breaking changes are backward-compatible and don't require consumer updates.

### What Constitutes Non-Breaking Changes

#### Additions
- **New storage keys**: Adding new persistent data
- **New public functions**: Adding new callable functions
- **New events**: Adding new event emissions
- **New query functions**: Adding new read-only operations

#### Improvements
- **Performance optimizations**: Making operations faster without changing behavior
- **Gas optimizations**: Reducing execution costs
- **Error handling improvements**: Better error messages or handling
- **Security enhancements**: Adding protections without changing interface

#### Bug Fixes
- **Logic corrections**: Fixing incorrect behavior
- **Edge case handling**: Handling previously unconsidered scenarios
- **Calculation fixes**: Correcting mathematical errors

### Non-Breaking Change Examples

```rust
// ✅ NON-BREAKING: Adding new storage key
const METADATA_KEY: Symbol = symbol_short!("METADATA");

// ✅ NON-BREAKING: Adding new function
pub fn get_contract_info(env: Env) -> (Address, u32) { ... }

// ✅ NON-BREAKING: Adding new event
env.events().publish(("metadata_updated", id), (metadata,));

// ✅ NON-BREAKING: Performance optimization
let result = cached_value.unwrap_or_else(|| compute_expensive_value());
```

## Change Management Process

### Pre-Change Assessment

1. **Impact Analysis**: Evaluate if the change is breaking or non-breaking
2. **Consumer Impact**: Identify affected integrations
3. **Migration Planning**: Plan required migrations for breaking changes
4. **Testing Strategy**: Design comprehensive tests

### Implementation Steps

#### For Non-Breaking Changes
1. Implement change
2. Add comprehensive tests
3. Update CHANGELOG.md
4. Deploy using standard process
5. Monitor for issues

#### For Breaking Changes
1. **Announcement**: Communicate planned changes to consumers
2. **Implementation**: Develop new version with migration path
3. **Testing**: Extensive testing on testnet
4. **Documentation**: Update all documentation
5. **Migration Support**: Provide tools and guides
6. **Gradual Rollout**: Consider phased deployment
7. **Monitoring**: Close monitoring post-deployment

### Version Bumping Rules

#### Automatic Version Bumps
- **Patch version**: Incremented automatically for bug fixes
- **Minor version**: Incremented for new features
- **Major version**: Incremented for breaking changes

#### Manual Override
In special cases, version numbers may be manually adjusted:
- Security patches may warrant patch version even for interface changes
- Major refactoring may use major version even if technically compatible

## Communication Strategy

### Pre-Release Communication
- **Breaking changes**: Minimum 2 weeks notice
- **Major features**: 1 week notice recommended
- **Security patches**: Immediate release with clear documentation

### Release Communication
- **Release notes**: Detailed changelog
- **Migration guides**: Step-by-step instructions
- **Code examples**: Updated integration examples
- **Support channels**: Available help for migration

### Post-Release Support
- **Compatibility period**: Support old versions for reasonable period
- **Migration assistance**: Help consumers upgrade
- **Issue monitoring**: Track and address post-release issues

## Backend Integration Requirements

### Version Detection
Backend services must:
1. Check contract version on startup
2. Log version information
3. Warn if running outdated version
4. Implement compatibility checks

### Example Backend Implementation
```rust
fn check_contract_version(contract_address: &Address) -> Result<(), Error> {
    let current_version = contract_client.get_version()?;
    let supported_version = get_minimum_supported_version();
    
    if current_version < supported_version {
        log::warn!("Contract version {} is below minimum supported {}", 
                   current_version, supported_version);
        return Err(Error::UnsupportedVersion);
    }
    
    if current_version > get_latest_tested_version() {
        log::warn!("Contract version {} is newer than latest tested {}", 
                   current_version, get_latest_tested_version());
    }
    
    Ok(())
}
```

## Upgrade Tools and Automation

### Scripts Provided
- **`scripts/upgrade.sh`**: Automated contract upgrade script
- **Version validation**: Ensures proper version increments
- **Migration assistance**: Helps with data migration
- **Git tagging**: Automatic version tagging

### CI/CD Integration
- **Automated testing**: Test all version compatibility
- **Version checks**: Validate version numbering
- **Documentation generation**: Auto-generate changelogs

## Rollback Strategy

### When to Rollback
- **Critical bugs**: Issues affecting core functionality
- **Security vulnerabilities**: Exploitable security issues
- **Performance problems**: Severe performance degradation

### Rollback Process
1. **Assessment**: Evaluate rollback necessity
2. **Communication**: Notify stakeholders
3. **Execution**: Use upgrade script to deploy previous version
4. **Verification**: Test rollback thoroughly
5. **Documentation**: Document rollback and lessons learned

## Best Practices

### Development
- **Semantic versioning**: Strict adherence to SemVer
- **Backward compatibility**: Prioritize compatibility when possible
- **Comprehensive testing**: Test all version scenarios
- **Clear documentation**: Document all changes thoroughly

### Deployment
- **Gradual rollout**: Deploy to testnet first
- **Monitoring**: Watch for issues post-deployment
- **Backup plans**: Have rollback strategies ready
- **Communication**: Keep stakeholders informed

### Maintenance
- **Version tracking**: Maintain accurate version history
- **Deprecation policy**: Clear timeline for phasing out old versions
- **Support lifecycle**: Define support periods for each version

## Compliance and Auditing

### Requirements
- **Change tracking**: All changes must be documented
- **Version history**: Maintain complete version history
- **Audit trail**: Keep records of all deployments
- **Regulatory compliance**: Ensure changes meet regulatory requirements

### Review Process
- **Code review**: All changes must be reviewed
- **Architecture review**: Major changes require architecture approval
- **Security review**: Breaking changes require security assessment
- **Compliance review**: Ensure regulatory compliance

---

This policy ensures that contract evolution is managed responsibly, maintaining trust while enabling innovation and improvement.
