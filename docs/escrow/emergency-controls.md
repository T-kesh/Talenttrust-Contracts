# Two-tier pause model: Soft Pause vs Emergency Pause

This document describes the intended **two-tier pause controls** for the escrow contract.

## Overview

The contract supports two pause tiers:

1. **Soft Pause** (`PauseTier::SoftPaused`)

   - Goal: temporarily restrict normal contract operations.
   - In this simplified implementation, it is treated as a state toggle managed by the admin.

2. **Emergency Pause** (`PauseTier::EmergencyPaused`)
   - Goal: halt additional/critical paths in response to emergencies.
   - In this simplified implementation, it is treated as a stronger state toggle.

The contract also supports:

- **Unpause** (`unpause_all`)
- **Resolve emergency reason** (`resolve_emergency_reason`) to allow unpausing after an emergency.

## Admin model

- The contract stores a single `admin` address inside `PauseState`.
- Only the stored admin can call:
  - `pause_soft`
  - `pause_emergency`
  - `resolve_emergency_reason`
  - `unpause_all`

## Unpause rules

- If the contract is currently in **Emergency Pause**, then `unpause_all` will only succeed when:
  - `emergency_reason_resolved == true`
- Once resolved, the admin can unpause back to `PauseTier::Unpaused`.

## Edge cases covered by tests

- **Unpause during emergency** is rejected until the emergency reason is resolved.
- **Double unpause** is treated as idempotent (subsequent `unpause_all` calls from admin do not panic).
- **Non-admin caller** cannot unpause.
