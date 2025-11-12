# Galaxy Facilitator Vault Program

This is the on-chain "Vault" program for the Galaxy Facilitator protocol, a system for x402-style continuous, metered payments on Solana.[9, 10, 11]

This program is written in Rust using the [Anchor framework](https://www.anchor-lang.com/). [12, 13]

## Overview

The program acts as a non-custodial "budget vault."

  * Users deposit SPL tokens into a secure, personal Program-Derived Address (PDA) vault. [14, 15, 16]
  * A trusted off-chain server (the "agent" or "Facilitator") is given permission to make programmatic withdrawals from this vault.
  * The user always retains the ability to withdraw all remaining funds and close their vault at any time. [17, 18, 19]

## On-Chain Components

### Account State

  * `PaymentVault`: A PDA (seeds: `[b"vault", user.key()]`) that stores the metadata for the vault, including the user's public key (`authority`), the server's public key (`agent`), and the token `mint`.

### Instructions

1.  **`initialize_vault`**: Called by the **user**. Creates and funds the `PaymentVault` PDA and its associated token account.
2.  **`spend_from_vault`**: Called by the **agent** (server). Transfers a specified `amount` of tokens from the user's `TokenVault` to the server's `TreasuryTokenAccount`. This instruction is heavily secured with `has_one` constraints to ensure the signer is the designated agent. [3, 4, 5]
3.  **`withdraw_and_close`**: Called by the **user**. Transfers 100% of the remaining tokens from the `TokenVault` back to the user and closes both on-chain accounts, refunding the rent. [8]

## Prerequisites

Before you begin, ensure you have the following tools installed:

  * Rust (via `rustup`) [20]
  * Solana CLI
  * Anchor Framework (`avm install latest` and `avm use latest`) [12]

## 1\. Critical Configuration (MUST DO)

This program's security relies on hardcoding the "agent" (your server's) public key.

1.  **Generate a Keypair for your Server:**
    This keypair will be used by your Node.js server to sign the `spend_from_vault` transactions. [21, 22, 23]bash
    solana-keygen new --outfile./agent-keypair.json

    ```
    *Keep this `agent-keypair.json` file secure!* This is your server's private key.

    ```

2.  **Get the Server's Public Key:**

    ```bash
    solana-keygen pubkey./agent-keypair.json
    # It will output a public key, e.g., "Agentp81aN1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF"
    ```

3.  **Update the Rust Code:**
    Open `programs/galaxy_facilitator/src/lib.rs` and find the `initialize_vault` function. Replace the placeholder public key with your server's public key from Step 2.

    ```rust
    //... inside initialize_vault...

    //! CRITICAL: Replace this with your server's actual wallet public key.
    const AGENT_PUBKEY: Pubkey = pubkey!("Agentp81aN1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF");

    //...
    ```

## 2\. Build, Test, and Deploy

Once you have configured the `AGENT_PUBKEY`, you can proceed with the standard Anchor workflow.

1.  **Build the Program:**
    This will compile the Rust code, check for errors, and generate the program's IDL (Interface Definition Language).

    ```bash
    anchor build
    ```

2.  **Run Local Tests (Optional but Recommended):**
    Start a local validator in one terminal:

    ```bash
    anchor localnet
    ```

    In a second terminal, run the tests:

    ```bash
    anchor test
    ```

3.  **Deploy to a Cluster (Devnet or Mainnet):**
    First, make sure your Solana CLI is configured for the desired cluster (e.g., `solana config set --url devnet`). Ensure your wallet has enough SOL to cover deployment fees.

    Then, deploy the program:

    ```bash
    anchor deploy
    ```

## 3\. Update Program ID

After deployment, the CLI will output a new `Program ID`.

  * Copy this new Program ID.
  * Paste it into `programs/galaxy_facilitator/src/lib.rs` at the `declare_id!(...)` macro.
  * Paste it into `Anchor.toml` under the `[programs.localnet]` (or `devnet`/`mainnet`) section. [24]
  * Run `anchor build` one more time to lock in the new Program ID.

## 4\. Using the IDL

After a successful build, the program's "ABI" (called an IDL on Solana) is generated at `target/idl/galaxy_facilitator.json`.

You will need to **copy this JSON file into your Node.js "Galaxy Facilitator" server project.** The `@coral-xyz/anchor` client library will use this IDL to format and send transactions to your on-chain program. [25, 26]

```
```
