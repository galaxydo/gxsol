use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

// 1. Program ID
// This is a placeholder. After you deploy, Anchor will give you the
// real program ID. You must paste it here and in Anchor.toml.
declare_id!("GXYFac1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF9n1jF");

#[program]
pub mod galaxy_facilitator {
    use super::*;

    // Instruction 1: Called by the user to create and fund the vault
    pub fn initialize_vault(ctx: Context<InitializeVault>, amount: u64) -> Result<()> {
        // Hardcode the "agent" (server) public key for security.
        //! CRITICAL: Replace this with your server's actual wallet public key.
        const AGENT_PUBKEY: Pubkey = pubkey!("AgentWalletPublicKeyGoesHere");

        // Set the metadata on the new PaymentVault PDA
        let vault = &mut ctx.accounts.payment_vault;
        vault.authority = ctx.accounts.user.key();
        vault.agent = AGENT_PUBKEY;
        vault.mint = ctx.accounts.mint.key();
        vault.bump = ctx.bumps.payment_vault;

        // Perform the initial deposit transfer
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_token_account.to_account_info(),
            to: ctx.accounts.token_vault.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
        
        token::transfer(cpi_context, amount)?;

        Ok(())
    }

    // Instruction 2: Called by the server ("agent") to spend from the vault
    pub fn spend_from_vault(ctx: Context<SpendFromVault>, amount: u64) -> Result<()> {
        // Define the PDA seeds for signing the CPI [1]
        let authority_key = ctx.accounts.authority.key();
        let seeds = &[
            b"vault",
            authority_key.as_ref(),
            &[ctx.accounts.payment_vault.bump];
        let signer_seeds = &[&seeds[..]];

        // Create the CPI accounts for token transfer
        let cpi_accounts = Transfer {
            from: ctx.accounts.token_vault.to_account_info(),
            to: ctx.accounts.treasury_token_account.to_account_info(),
            authority: ctx.accounts.payment_vault.to_account_info(), // PDA is authority
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();

        // Create the CpiContext *with signer* [1]
        let cpi_context = CpiContext::new_with_signer(
            cpi_program,
            cpi_accounts,
            signer_seeds
        );

        // Execute the PDA-signed transfer
        token::transfer(cpi_context, amount)?;

        Ok(())
    }

    // Instruction 3: Called by the user to reclaim all funds and close
    pub fn withdraw_and_close(ctx: Context<WithdrawAndClose>) -> Result<()> {
        // Get total remaining amount from the vault
        let amount = ctx.accounts.token_vault.amount;
        if amount == 0 {
            msg!("No tokens to withdraw. Closing accounts.");
            // Accounts will still be closed by Anchor due to the 'close' constraint
            return Ok(());
        }

        // Define PDA signer seeds
        let authority_key = ctx.accounts.authority.key();
        let seeds = &[
            b"vault",
            authority_key.as_ref(),
            &[ctx.accounts.payment_vault.bump];
        let signer_seeds = &[&seeds[..]];

        // Create CPI context to transfer *all* remaining tokens
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

        // Execute the final transfer
        token::transfer(cpi_context, amount)?;

        // Anchor handles the account closing automatically
        // due to the 'close = authority' constraint.
        Ok(())
    }
}

// -----------------------------------------------------------------
// 2. Account Structs (State)
// -----------------------------------------------------------------

// The metadata PDA that acts as the authority
// Seeds: [b"vault", authority.key().as_ref()]
#[account]
pub struct PaymentVault {
    pub authority: Pubkey, // The user's wallet
    pub agent: Pubkey,     // The "Galaxy Facilitator" server wallet
    pub mint: Pubkey,      // The mint of the token being stored
    pub bump: u8,          // The canonical bump seed
}

// -----------------------------------------------------------------
// 3. Instruction Contexts (Account Validation)
// -----------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    // 1. Create the PaymentVault PDA (stores metadata)
    #[account(
        init,
        payer = user,
        space = 8 + 32 + 32 + 32 + 1, // 105 bytes
        seeds = [b"vault", user.key().as_ref()],
        bump
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 2. Create the TokenVault ATA (stores tokens)
    //    Its authority is set to the PaymentVault PDA [2]
    #[account(
        init,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = payment_vault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 3. The user (Signer) who is paying for creation and depositing
    #[account(mut)]
    pub user: Signer<'info>,

    // 4. The user's existing token account to pull the deposit from
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = user
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
#[instruction(amount: u64)]
pub struct SpendFromVault<'info> {
    // 1. The metadata PDA. Note the powerful constraints.
    #[account(
        mut,
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority, // Checks payment_vault.authority == authority.key() 
        has_one = agent,     // Checks payment_vault.agent == agent.key() 
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 2. The token vault ATA, owned by the PDA
    #[account(
        mut,
        associated_token::mint = payment_vault.mint,
        associated_token::authority = payment_vault
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 3. The user's wallet. MUST be provided, but NOT a signer.
    //    'has_one' validates its relationship to payment_vault.
    /// CHECK: This is safe because has_one constraint verifies it. [6]
    pub authority: AccountInfo<'info>,

    // 4. The "Galaxy Facilitator" server. MUST be the signer.
    //    'has_one' validates it's the *correct* signer. [7, 3]
    #[account(mut)]
    pub agent: Signer<'info>,

    // 5. The service's treasury wallet (where the money goes)
    #[account(
        mut,
        associated_token::mint = payment_vault.mint
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    // 6. Required programs
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct WithdrawAndClose<'info> {
    // 1. The metadata PDA. 'close = authority' refunds rent to the user. 
    #[account(
        mut,
        seeds = [b"vault", authority.key().as_ref()],
        bump = payment_vault.bump,
        has_one = authority,
        close = authority
    )]
    pub payment_vault: Account<'info, PaymentVault>,

    // 2. The token vault. 'close = authority' refunds rent to the user. 
    #[account(
        mut,
        associated_token::mint = payment_vault.mint,
        associated_token::authority = payment_vault,
        close = authority
    )]
    pub token_vault: Account<'info, TokenAccount>,

    // 3. The user. MUST be the signer.
    #[account(mut)]
    pub authority: Signer<'info>,

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
// 4. Custom Errors
// -----------------------------------------------------------------
#[error_code]
pub enum ErrorCode {
    #
    InvalidAgent,
}
