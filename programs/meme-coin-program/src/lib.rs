use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};
use anchor_spl::associated_token::AssociatedToken;

declare_id!("5ZCsDZAV9oH7Souj6UWtX3Q94ZrmPkVF5MVQuzmDd66X");// This will be replaced when you deploy

#[program]
pub mod meme_coin_program {
    use super::*;

    // Initialize a new meme coin
    pub fn create_meme_coin(
        ctx: Context<CreateMemeCoin>,
        name: String,
        symbol: String,
        uri: String,
        decimals: u8,
        initial_supply: u64,
        price_per_token: u64, // Price in lamports per token
    ) -> Result<()> {
        let meme_coin = &mut ctx.accounts.meme_coin;
        
        // Clone values before moving them for the message
        let name_clone = name.clone();
        let symbol_clone = symbol.clone();
        
        meme_coin.creator = ctx.accounts.creator.key();
        meme_coin.mint = ctx.accounts.mint.key();
        meme_coin.name = name;
        meme_coin.symbol = symbol;
        meme_coin.uri = uri;
        meme_coin.decimals = decimals;
        meme_coin.total_supply = initial_supply;
        meme_coin.price_per_token = price_per_token;
        meme_coin.is_active = true;
        meme_coin.total_volume = 0;
        meme_coin.holders_count = 1; // Creator is first holder
        
        msg!("Meme coin created: {} ({})", name_clone, symbol_clone);
        Ok(())
    }

    // Buy meme coin with SOL
    pub fn buy_meme_coin(
        ctx: Context<BuyMemeCoin>,
        amount: u64, // Amount of tokens to buy
    ) -> Result<()> {
        let meme_coin = &mut ctx.accounts.meme_coin;
        
        require!(meme_coin.is_active, ErrorCode::CoinNotActive);
        
        // Calculate total cost in lamports
        let total_cost = amount
            .checked_mul(meme_coin.price_per_token)
            .ok_or(ErrorCode::Overflow)?;
        
        // Transfer SOL from buyer to creator
        let transfer_instruction = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.buyer.key(),
            &ctx.accounts.creator.key(),
            total_cost,
        );
        
        anchor_lang::solana_program::program::invoke(
            &transfer_instruction,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.creator.to_account_info(),
            ],
        )?;

        // Mint tokens to buyer
        let cpi_accounts = token::MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.buyer_token_account.to_account_info(),
            authority: ctx.accounts.creator.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        
        token::mint_to(cpi_ctx, amount)?;

        // Update meme coin stats
        meme_coin.total_volume = meme_coin.total_volume
            .checked_add(total_cost)
            .ok_or(ErrorCode::Overflow)?;
        
        msg!("Bought {} tokens for {} lamports", amount, total_cost);
        Ok(())
    }

    // Sell meme coin for SOL
    pub fn sell_meme_coin(
        ctx: Context<SellMemeCoin>,
        amount: u64,
    ) -> Result<()> {
        let meme_coin = &mut ctx.accounts.meme_coin;
        
        require!(meme_coin.is_active, ErrorCode::CoinNotActive);
        
        // Calculate SOL to receive (with small fee)
        let sol_amount = amount
            .checked_mul(meme_coin.price_per_token)
            .ok_or(ErrorCode::Overflow)?
            .checked_mul(95) // 5% fee
            .ok_or(ErrorCode::Overflow)?
            .checked_div(100)
            .ok_or(ErrorCode::Overflow)?;
        
        // Burn tokens from seller
        let cpi_accounts = token::Burn {
            mint: ctx.accounts.mint.to_account_info(),
            from: ctx.accounts.seller_token_account.to_account_info(),
            authority: ctx.accounts.seller.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        
        token::burn(cpi_ctx, amount)?;

        // Transfer SOL to seller
        **ctx.accounts.creator.try_borrow_mut_lamports()? -= sol_amount;
        **ctx.accounts.seller.try_borrow_mut_lamports()? += sol_amount;

        // Update stats
        meme_coin.total_volume = meme_coin.total_volume
            .checked_add(sol_amount)
            .ok_or(ErrorCode::Overflow)?;
        
        msg!("Sold {} tokens for {} lamports", amount, sol_amount);
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct CreateMemeCoin<'info> {
    #[account(
        init,
        payer = creator,
        space = 8 + MemeCoin::INIT_SPACE,
        seeds = [b"meme_coin", name.as_bytes()],
        bump
    )]
    pub meme_coin: Account<'info, MemeCoin>,
    
    #[account(
        init,
        payer = creator,
        mint::decimals = 9,
        mint::authority = creator,
    )]
    pub mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub creator: Signer<'info>,
    
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct BuyMemeCoin<'info> {
    #[account(mut)]
    pub meme_coin: Account<'info, MemeCoin>,
    
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub buyer: Signer<'info>,
    
    /// CHECK: This is the creator account, validated by meme_coin.creator
    #[account(mut, constraint = creator.key() == meme_coin.creator)]
    pub creator: AccountInfo<'info>,
    
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = mint,
        associated_token::authority = buyer,
    )]
    pub buyer_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SellMemeCoin<'info> {
    #[account(mut)]
    pub meme_coin: Account<'info, MemeCoin>,
    
    #[account(mut)]
    pub mint: Account<'info, Mint>,
    
    #[account(mut)]
    pub seller: Signer<'info>,
    
    /// CHECK: This is the creator account, validated by meme_coin.creator
    #[account(mut, constraint = creator.key() == meme_coin.creator)]
    pub creator: AccountInfo<'info>,
    
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = seller,
    )]
    pub seller_token_account: Account<'info, TokenAccount>,
    
    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(InitSpace)]
pub struct MemeCoin {
    pub creator: Pubkey,
    pub mint: Pubkey,
    #[max_len(50)]
    pub name: String,
    #[max_len(10)]
    pub symbol: String,
    #[max_len(200)]
    pub uri: String, // Metadata URI
    pub decimals: u8,
    pub total_supply: u64,
    pub price_per_token: u64, // Price in lamports
    pub is_active: bool,
    pub total_volume: u64,
    pub holders_count: u32,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Meme coin is not active")]
    CoinNotActive,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Insufficient funds")]
    InsufficientFunds,
}