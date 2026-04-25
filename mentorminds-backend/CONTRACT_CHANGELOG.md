# Soroban Escrow Contract Changelog

This document tracks backend-expected Soroban escrow contract versions and ABI assumptions.

## Version `v1.0.0`
- `create_escrow(escrow_id, mentor, learner, amount)`
- `open_dispute(escrow_id, reason)`
- `resolve_dispute(escrow_id, split_percentage)`
- `release_funds(escrow_id)`
- `refund(escrow_id)`
- Optional: `get_version() -> string`

## Operational Notes
- Backend must set `SOROBAN_CONTRACT_VERSION` to the ABI version it is built against.
- On startup (or bootstrap), backend should call `verifyContractVersion()` and disable Soroban integration when versions diverge.
- Escrow records persist `sorobanContractVersion` for auditability.
