# Galaxy Facilitator: On-Chain Metered Billing Protocol

This repository contains the on-chain Anchor program for the **Galaxy Facilitator Protocol**, a non-custodial system for x402-style metered billing on Solana. [1, 2, 3, 4, 5, 6, 7, 8, 9]

It enables users to deposit funds into a personal vault and delegate spending authority to one or more "agents" (e.g., API servers, AI agents) with specific, on-chain enforced budgets. This allows for high-frequency, off-chain authorized payments without requiring the user to sign every transaction.

## Core Concepts & Architecture

The protocol operates on a "leashed trust" model. The user (Authority) provisions a budget, and the agent (Server) can draw from that budget up to the specified limit. The user *always* retains custody of their funds.

### Key Components

1.  **`PaymentVault` (PDA)**

      * **Seeds**: `[b"vault", authority.key()]`
      * **Description**: This is the user's master vault. It is a Program-Derived Address that serves as the central authority for the user's funds. It *owns* the `TokenVault`. [10, 11, 12]

2.  **`TokenVault` (ATA)**

      * **Authority**: The `PaymentVault` PDA.
      * **Description**: This is an Associated Token Account that holds the user's *actual* SPL tokens (e.g., USDC). All agent spending is drawn from this single account. [13, 14, 15]

3.  **`AgentPermission` (PDA)**

      * **Seeds**: `[b"permission", authority.key(), agent.key()]`
      * **Description**: This is the "leash." It is a separate metadata PDA that links a user (`authority`) to a specific `agent`. It stores the `budget` (total allowance) and `spent` (total consumed) for that agent. This account is the core of the multi-agent budget system. [16]

## Protocol Flow

The system is designed for a clean separation of responsibilities between the user and the agent.

1.  **Funding (User)**: A user calls `initialize_vault` one time. This creates their `PaymentVault`, the associated `TokenVault`, and makes an initial deposit. [10, 17]
2.  **Delegation (User)**: The user calls `authorize_agent` for *each* service (agent) they wish to use. This instruction creates (or updates) an `AgentPermission` PDA, setting a `budget` for that specific agent.
3.  **Spending (Agent)**:
      * A user authenticates with the agent's off-chain server (e.g., by signing an off-chain message). [18, 19, 20]
      * The agent's server, now authorized, calls `spend_from_vault` to request payment for its service.
      * The on-chain program verifies:
        1.  The *signer* is the `agent` specified in the `AgentPermission` PDA. [21, 22, 23, 24]
        2.  The requested `amount` is within the `budget - spent` limit.
      * If valid, the program executes a CPI transfer from the user's `TokenVault` (signed by the `PaymentVault` PDA) to the agent's treasury wallet. [10, 25, 26]
4.  **Revoking (User)**: The user calls `revoke_agent` to destroy an agent's `AgentPermission` account, instantly revoking all spending privileges and refunding the account's rent.
5.  **Exiting (User)**: The user calls `withdraw_and_close` at any time. This is the non-custodial exit ramp. It transfers 100% of the remaining funds from the `TokenVault` back to the user and closes both the `PaymentVault` and `TokenVault` accounts. [27, 28, 29, 30]

## Instruction API Reference

### User-Facing Instructions

**1. `initialize_vault(ctx, amount: u64)`**

  * **Signer**: `authority` (User)
  * **Description**: Creates the user's master `PaymentVault` and its `TokenVault`. Deposits an initial `amount` of tokens.

**2. `authorize_agent(ctx, budget: u64)`**

  * **Signer**: `authority` (User)
  * **Description**: Creates or updates an `AgentPermission` PDA for a specific `agent`. Sets the total `budget` this agent is authorized to spend. Uses `init_if_needed` for seamless creation or updates. [22]

**3. `revoke_agent(ctx)`**

  * **Signer**: `authority` (User)
  * **Description**: Closes an `AgentPermission` account, revoking the `agent`'s spending rights and refunding the account's rent to the `authority`.

**4. `withdraw_and_close(ctx)`**

  * **Signer**: `authority` (User)
  * **Description**: The non-custodial exit. Withdraws all remaining funds from the `TokenVault` and closes the master vault accounts, refunding all rent.

### Agent-Facing Instructions

**5. `spend_from_vault(ctx, amount: u64)`**

  * **Signer**: `agent` (Server)
  * **Description**: Called by the agent to request payment. The program validates the request against the `AgentPermission` budget and, if successful, transfers `amount` of tokens from the user's `TokenVault` to the agent's `treasury_token_account`.

## Security Model

Security is enforced through Anchor's constraint system, preventing unauthorized access or spending. [31, 32]

  * **`has_one`**: This is the primary constraint. `spend_from_vault` uses `has_one = authority` and `has_one = agent` on the `AgentPermission` account. This cryptographically ensures that the `agent` signing the transaction is the *exact* agent authorized by the `authority` (user) for that specific permission account. [21, 22, 23, 24]
  * **`seeds` & `bump`**: All PDA accounts (`PaymentVault`, `AgentPermission`) are validated with their seeds and canonical bump. This prevents PDA substitution attacks, where an attacker might pass a malicious account. [16, 33]
  * **Budget Enforcement**: All spending logic is on-chain and atomic. The check for `amount <= budget - spent` and the update to `spent` happen in the same instruction, making it impossible for an agent to overspend their budget.
  * **Non-Custodial**: At no point does the agent or the protocol take custody of the user's funds. The `withdraw_and_close` instruction guarantees the user can reclaim their entire balance at any time. [30]

## Development & Deployment

### Prerequisites

  * ([https://www.rust-lang.org/tools/install](https://www.rust-lang.org/tools/install))
  * ([https://docs.solana.com/cli/install](https://docs.solana.com/cli/install))
  * [Anchor Framework](https://www.anchor-lang.com/docs/installation) (`avm install latest` && `avm use latest`) [34]

### Build

Compile the program and generate the IDL (Interface Definition Language):

```bash
anchor build
```

The IDL will be generated at `target/idl/galaxy_facilitator.json`.

### Test

Run the local test suite:

```bash
anchor test
```

### Deploy

1.  Configure your Solana CLI for the desired cluster (e.g., `devnet` or `mainnet-beta`):

    ```bash
    solana config set --url devnet
    ```

2.  Ensure your deployment wallet is funded.

    ```bash
    solana airdrop 2 # (Devnet only)
    ```

3.  Deploy the program:

    ```bash
    anchor deploy
    ```

### Post-Deployment Configuration

The `anchor deploy` command will output a new Program ID.

1.  **Update `lib.rs`**: Copy the new Program ID and paste it into the `declare_id!(...)` macro at the top of `programs/galaxy_facilitator/src/lib.rs`.
2.  **Update `Anchor.toml`**: Paste the new Program ID into your `Anchor.toml` file, under the cluster you deployed to (e.g., `[programs.devnet]`).

Re-run `anchor build` one final time to incorporate the new Program ID into the IDL.

## Off-Chain Integration

This on-chain program serves as the trust layer. The "Galaxy Facilitator" server is the off-chain component responsible for verifying user authorization (e.g., via `signMessage`) and calling the `spend_from_vault` instruction.

The server will use the generated `target/idl/galaxy_facilitator.json` and the `@coral-xyz/anchor` Typescript library to build, sign (as the `agent` and `feePayer`), and send transactions. [35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47]
