use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

// This Program ID is a placeholder.
// After deployment, replace it with the new Program ID in Anchor.toml
// and in this declare_id! macro.
declare_id!("GXYFac1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF");

#[program]
pub mod galaxy_facilitator {
    use super::*;

    /// (USER) Instruction 1: Creates the master PaymentVault and TokenVault.
    /// This is the user's central, non-custodial fund.
    pub fn initialize_vault(ctx: Context<InitializeVault>, amount: u64) -> Result<()> {
        // 1. Set the metadata on the new PaymentVault PDA
        let vault = &mut ctx.accounts.payment_vault;
        vault.authority = ctx.accounts.authority.key();
        vault.mint = ctx.accounts.mint.key();
        vault.bump = ctx.bumps.payment_vault;

        // 2. Perform the initial deposit transfer if amount > 0
        if amount > 0 {
            let cpi_accounts = Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.token_vault.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
            
            token::transfer(cpi_context, amount)?;
        }

        Ok(())
    }

    /// (USER) Instruction 2: Authorizes a specific agent with a specific budget.
    /// Uses `init_if_needed` to create or update an agent's permission.
    pub fn authorize_agent(ctx: Context<AuthorizeAgent>, budget: u64) -> Result<()> {
        let permission = &mut ctx.accounts.agent_permission;
        
        permission.authority = ctx.accounts.authority.key();
        permission.agent = ctx.accounts.agent.key();
        permission.budget = budget;
        permission.bump = ctx.bumps.agent_permission;
        
        // If the account is being initialized, set 'spent' to 0.
        // If it's being updated, 'spent' persists, allowing for
        // budget increases or decreases while tracking existing spending.
        // A user can reset the 'spent' amount by calling 'revoke_agent' first.
        if permission.spent == 0 {
            permission.spent = 0;
        }

        Ok(())
    }

    /// (USER) Instruction 3: Revokes an agent's permission.
    /// Closes the permission account and refunds the rent to the user.
    pub fn revoke_agent(_ctx: Context<RevokeAgent>) -> Result<()> {
        // Anchor's 'close' constraint handles the rent refund and account closure.
        Ok(())
    }

    /// (AGENT) Instruction 4: Called by the server ("agent") to spend from the vault.
    /// This is the core instruction for metered billing.
    pub fn spend_from_vault(ctx: Context<SpendFromVault>, amount: u64) -> Result<()> {
        // 1. Check if the requested amount exceeds the agent's remaining budget.
        let permission = &mut ctx.accounts.agent_permission;
        let remaining_budget = permission.budget
           .checked_sub(permission.spent)
           .ok_or(ErrorCode::MathOverflow)?;

        if amount > remaining_budget {
            return err!(ErrorCode::BudgetExceeded);
        }

        // 2. Update the agent's 'spent' amount.
        permission.spent = permission.spent
           .checked_add(amount)
           .ok_or(ErrorCode::MathOverflow)?;

        // 3. Define the PDA seeds for signing the CPI
        let authority_key = ctx.accounts.authority.key();
        let seeds = &[
            b"vault",
            authority_key.as_ref(),
            &[ctx.accounts.payment_vault.bump];
        let signer_seeds = &[&seeds[..]];

        // 4. Create the CPI accounts for token transfer
        let cpi_accounts = Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.payment_vault.to_account_info(), // PDA is authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();

        // 5. Create the CpiContext *with signer*
        let cpi_context = CpiContext::new_with_signer(
            cpi_program,
            cpi_accounts,
            signer_seeds
        );

        // 6. Execute the PDA-signed transfer
        token::transfer(cpi_context, amount)?;

        Ok(())
    }

    /// (USER) Instruction 5: User withdraws all funds and closes the master vault.
    /// This is the non-custodial "exit ramp."
    pub fn withdraw_and_close(ctx: Context<WithdrawAndClose>) -> Result<()> {
        // 1. Get total remaining amount from the vault
        let amount = ctx.accounts.token_vault.amount;
        if amount == 0 {
            msg!("No tokens to withdraw. Closing accounts.");
            // Accounts will still be closed by Anchor.
            return Ok(());
        }

        // 2. Define PDA signer seeds
        let authority_key = ctx.accounts.authority.key();
        let seeds = &[
            b"vault",
            authority_key.as_ref(),
            &[ctx.accounts.payment_vault.bump];
        let signer_seeds = &[&seeds[..]];

        // 3. Create CPI context to transfer *all* remaining tokens
        let cpi_accounts = Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.payment_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_context = CpiContext::new_with_signer(
            cpi_program,
            cpi_accounts,
            signer_seeds
        );

        // 4. Execute the final transfer
        token::transfer(cpi_context, amount)?;

        // 5. Anchor handles account closing via the 'close' constraint.
        Ok(())
    }
}

// -----------------------------------------------------------------
// 1. Account Structs (State)
// -----------------------------------------------------------------

/// The user's master vault. Holds no tokens itself, but acts as the
/// authority for the `TokenVault`.
/// Seeds: [b"vault", authority.key().as_ref()]
#[account]
pub struct PaymentVault {
    pub authority: Pubkey, // The user's wallet
    pub mint: Pubkey,      // The mint of the token being stored
    pub bump: u8,          // The canonical bump seed
}

/// The permission "leash" for a specific agent.
/// This account defines the budget for a single agent, authorized by the user.
/// Seeds: [b"permission", authority.key().as_ref(), agent.key().as_ref()]
#[account]
pub struct AgentPermission {
    pub authority: Pubkey, // The user's wallet
    pub agent: Pubkey,     // The "Galaxy Facilitator" server wallet
    pub budget: u64,       // Total budget authorized for this agent
    pub spent: u64,        // Total amount this agent has spent
    pub bump: u8,
}

// -----------------------------------------------------------------
// 2. Instruction Contexts (Account Validation)
// -----------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    // 1. The user (Signer) who is paying for creation and depositing
    #[account(mut)]
    pub authority: Signer<'info>,

    // 2. Create the PaymentVault PDA (stores metadata)
    #[account(
        init,
        payer = authority,
        space = 8 + 32 + 32 + 1, // 73 bytes
        seeds = [b"vault", authority.key().as_ref()],
        bump
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 3. Create the TokenVault ATA (stores tokens)
    //    Its authority is set to the PaymentVault PDA
    #[account(
        init,
        payer = authority,
        associated_token::mint = mint,
        associated_token::authority = payment_vault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 4. The user's existing token account to pull the deposit from
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = authority
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    // 5. Mint account, for validation
    pub mint: Account<'info, Mint>,
    
    // 6. Required programs
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AuthorizeAgent<'info> {
    // 1. The user (Signer)
    #[account(mut)]
    pub authority: Signer<'info>,

    // 2. The agent being authorized
    /// CHECK: This is safe as it's only used as a seed and stored.
    pub agent: AccountInfo<'info>,

    // 3. The user's master vault, used to validate the authority
    #[account(
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 4. The permission account, created if it doesn't exist
    #[account(
        init_if_needed,
        payer = authority,
        space = 8 + 32 + 32 + 8 + 8 + 1, // 89 bytes
        seeds = [b"permission", authority.key().as_ref(), agent.key().as_ref()],
        bump
    )]
    pub agent_permission: Account<'info, AgentPermission>,

    // 5. Required programs
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RevokeAgent<'info> {
    // 1. The user (Signer)
    #[account(mut)]
    pub authority: Signer<'info>,

    // 2. The agent being revoked
    /// CHECK: This is safe as it's only used as a seed for validation.
    pub agent: AccountInfo<'info>,

    // 3. The user's master vault, for authority validation
    #[account(
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 4. The permission account to be closed
    #[account(
        mut,
        seeds = [b"permission", authority.key().as_ref(), agent.key().as_ref()],
        bump = agent_permission.bump,
        has_one = authority,
        has_one = agent,
        close = authority
    )]
    pub agent_permission: Account<'info, AgentPermission>,
}

#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct SpendFromVault<'info> {
    // 1. The "Galaxy Facilitator" server (Signer)
    #[account(mut)]
    pub agent: Signer<'info>,

    // 2. The user's wallet. MUST be provided, but NOT a signer.
    /// CHECK: This is safe because 'has_one' constraints verify it.
    #[account(mut)]
    pub authority: AccountInfo<'info>,

    // 3. The user's master vault
    #[account(
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 4. The token vault ATA, owned by the PDA
    #[account(
        mut,
        associated_token::mint = payment_vault.mint,
        associated_token::authority = payment_vault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 5. The agent's permission account
    #[account(
        mut,
        seeds = [b"permission", authority.key().as_ref(), agent.key().as_ref()],
        bump = agent_permission.bump,
        has_one = authority,
        has_one = agent
    )]
    pub agent_permission: Account<'info, AgentPermission>,

    // 6. The agent's treasury wallet (where the money goes)
    #[account(
        mut,
        associated_token::mint = payment_vault.mint
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    // 7. Required programs
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct WithdrawAndClose<'info> {
    // 1. The user (Signer)
    #[account(mut)]
    pub authority: Signer<'info>,

    // 2. The metadata PDA. 'close = authority' refunds rent to the user.
    #[account(
        mut,
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority,
        close = authority
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 3. The token vault. 'close = authority' refunds rent to the user.
    #[account(
        mut,
        associated_token::mint = payment_vault.mint,
        associated_token::authority = payment_vault,
        close = authority
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 4. The user's token account to send funds back to
    #[account(
        mut,
        associated_token::mint = payment_vault.mint,
        associated_token::authority = authority
    )]
    pub user_token_account: Account<'info, TokenAccount>,
    
    // 5. Required programs
    pub token_program: Program<'info, Token>,
}


// -----------------------------------------------------------------
// 3. Custom Errors
// -----------------------------------------------------------------
#[error_code]
pub enum ErrorCode {
    #
    BudgetExceeded,
    #[msg("A mathematical operation resulted in an overflow or underflow.")]
    MathOverflow,
}
