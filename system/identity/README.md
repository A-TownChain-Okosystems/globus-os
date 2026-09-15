# GlobusOS Identity Service

Identity is the userspace authority for GlobusOS profiles, authentication sessions, wallet identity binding, and recovery orchestration.

## Responsibilities

- create and load GlobusOS user profiles;
- establish local authentication sessions;
- request wallet provisioning through the Wallet Service boundary;
- bind a wallet public identity to a GlobusOS User ID;
- orchestrate recovery from the 24-word wallet recovery phrase;
- enforce capability-mediated access to identity and wallet operations;
- prevent secret material from entering logs, telemetry, AI context, or ordinary IPC messages.

## Security boundary

The Identity Service does **not** expose private keys to applications. Applications request high-level operations; the Wallet Service performs protected signing after capability and policy checks.

```text
Application
   |
   | identity/sign request
   v
Identity Service
   |
   | capability check
   v
Wallet Service
   |
   | protected key operation
   v
ShivaCore capability boundary
```

## Registration

1. Create local User ID and profile.
2. Request wallet provisioning from the dedicated `atc-wallet` integration.
3. Receive public wallet identity only.
4. Create and authenticate the identity binding.
5. Complete recovery-phrase verification.
6. Persist encrypted local profile/wallet metadata.
7. Start the user session.

The recovery phrase and private key are never part of the profile record.

## Recovery

Recovery reconstructs the wallet from the user-supplied 24-word phrase and creates a new local authentication credential. The phrase exists only for the duration required by the recovery operation and must be zeroized as soon as practical.

## Status

Architecture/integration baseline. Production cryptography and hardware-backed key storage remain explicit implementation gates.
