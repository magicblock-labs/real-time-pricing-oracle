mod state;

use crate::state::UpdateData;
use anchor_lang::prelude::borsh::BorshSchema;
use anchor_lang::prelude::*;
use anchor_lang::require_keys_eq;
use core::mem::size_of;
use ephemeral_rollups_sdk::anchor::{commit, delegate, ephemeral};
use ephemeral_rollups_sdk::cpi::DelegateConfig;
use ephemeral_rollups_sdk::ephem::commit_and_undelegate_accounts;
use ephemeral_rollups_sdk::utils::close_pda;
use pyth_solana_receiver_sdk::price_update::{PriceFeedMessage, PriceUpdateV2, VerificationLevel};

declare_id!("PriCems5tHihc6UDXDjzjeawomAwBduWMGAi8ZUjppd");

#[cfg(not(feature = "test-mode"))]
const ORACLE_IDENTITY: Pubkey = pubkey!("MPUxHCpNUy3K1CSVhebAmTbcTCKVxfk9YMDcUP2ZnEA");
const SEED_PREFIX: &[u8] = b"price_feed";

#[ephemeral]
#[program]
pub mod ephemeral_oracle {
    use super::*;

    pub fn initialize_price_feed(
        ctx: Context<InitializePriceFeed>,
        _provider: String,
        _symbol: String,
        feed_id: [u8; 32],
        exponent: i32,
    ) -> Result<()> {
        let clock = Clock::get()?;
        let price_feed = &mut ctx.accounts.price_feed;

        price_feed.write_authority = ctx.accounts.payer.key();
        price_feed.posted_slot = 0;
        price_feed.verification_level = VerificationLevel::Full;
        price_feed.price_message = PriceFeedMessage {
            feed_id,
            ema_conf: 0,
            ema_price: 0,
            price: 0,
            conf: 0,
            exponent,
            prev_publish_time: clock.unix_timestamp,
            publish_time: clock.unix_timestamp,
        };
        Ok(())
    }

    pub fn update_price_feed(
        ctx: Context<UpdatePriceFeed>,
        _provider: String,
        update_data: UpdateData,
    ) -> Result<()> {
        ensure_oracle(&ctx.accounts.payer)?;

        let clock = Clock::get()?;
        let price_feed = &mut ctx.accounts.price_feed;

        let new_price: i64 = update_data.temporal_numeric_value.quantized_value as i64;
        let prev = price_feed.price_message;

        price_feed.posted_slot = clock.slot;
        price_feed.price_message = PriceFeedMessage {
            prev_publish_time: prev.publish_time,
            price: new_price,
            publish_time: clock.unix_timestamp,
            ..prev
        };
        price_feed.verification_level = VerificationLevel::Full;

        msg!("Price: {}", price_feed.price_message.price);
        msg!("Exponent: {}", price_feed.price_message.exponent);
        Ok(())
    }

    pub fn delegate_price_feed(
        ctx: Context<DelegatePriceFeed>,
        provider: String,
        symbol: String,
    ) -> Result<()> {
        ensure_oracle(&ctx.accounts.payer)?;

        ctx.accounts.delegate_price_feed(
            &ctx.accounts.payer,
            &[SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
            DelegateConfig::default(),
        )?;
        Ok(())
    }

    pub fn undelegate_price_feed(
        ctx: Context<UndelegatePriceFeed>,
        _provider: String,
        _symbol: String,
    ) -> Result<()> {
        ensure_oracle(&ctx.accounts.payer)?;

        commit_and_undelegate_accounts(
            &ctx.accounts.payer,
            vec![&ctx.accounts.price_feed.to_account_info()],
            &ctx.accounts.magic_context,
            &ctx.accounts.magic_program,
        )?;
        Ok(())
    }

    pub fn close_price_feed(
        ctx: Context<ClosePriceFeed>,
        _provider: String,
        _symbol: String,
    ) -> Result<()> {
        ensure_oracle(&ctx.accounts.payer)?;
        close_pda(
            &ctx.accounts.price_feed,
            &ctx.accounts.payer.to_account_info(),
        )?;
        Ok(())
    }

    pub fn sample(ctx: Context<Sample>) -> Result<()> {
        // Deserialize the price feed
        let data_ref = ctx.accounts.price_update.data.borrow();
        let price_update = PriceUpdateV2::try_deserialize_unchecked(&mut data_ref.as_ref())
            .map_err(Into::<Error>::into)?;

        // Reject if the update is older than 60 seconds
        let maximum_age: u64 = 60;

        // Feed id is the price_update account address
        let feed_id: [u8; 32] = ctx.accounts.price_update.key().to_bytes();

        let price = price_update.get_price_no_older_than(&Clock::get()?, maximum_age, &feed_id)?;

        msg!(
            "The price is ({} ± {}) * 10^-{}",
            price.price,
            price.conf,
            price.exponent
        );
        msg!(
            "The price is: {}",
            price.price as f64 * 10_f64.powi(-price.exponent)
        );
        msg!("Slot: {}", price_update.posted_slot);
        msg!("Message: {:?}", price_update.price_message);

        Ok(())
    }
}

/* -------------------- Accounts -------------------- */

#[derive(Accounts)]
#[instruction(provider: String, symbol: String, feed_id: [u8; 32], exponent: i32)]
pub struct InitializePriceFeed<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    // Allocate for the actual V3 struct, not V2
    #[account(
        init,
        payer = payer,
        space = 8 + size_of::<PriceUpdateV3>(),
        seeds = [SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
        bump
    )]
    pub price_feed: Account<'info, PriceUpdateV3>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(provider: String, update_data: UpdateData)]
pub struct UpdatePriceFeed<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(
        mut,
        seeds = [SEED_PREFIX, provider.as_bytes(), update_data.symbol.as_bytes()],
        bump
    )]
    pub price_feed: Account<'info, PriceUpdateV3>,
}

#[delegate]
#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct DelegatePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK: delegated PDA
    #[account(
        mut,
        del,
        seeds = [SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
        bump
    )]
    pub price_feed: AccountInfo<'info>,
}

#[commit]
#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct UndelegatePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK: undelegated PDA
    #[account(
        mut,
        seeds = [SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
        bump
    )]
    pub price_feed: AccountInfo<'info>,
}

#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct ClosePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK: PDA to close
    #[account(
        mut,
        seeds = [SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
        bump
    )]
    pub price_feed: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct Sample<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: external price update account
    pub price_update: AccountInfo<'info>,
}

/* -------------------- State -------------------- */

#[account]
#[derive(BorshSchema)]
pub struct PriceUpdateV3 {
    pub write_authority: Pubkey,
    pub verification_level: VerificationLevel,
    pub price_message: PriceFeedMessage,
    pub posted_slot: u64,
}

/* -------------------- Helpers & Errors -------------------- */

fn ensure_oracle(payer: &Signer) -> Result<()> {
    #[cfg(not(feature = "test-mode"))]
    require_keys_eq!(payer.key(), ORACLE_IDENTITY, OracleError::Unauthorized);
    Ok(())
}

#[error_code]
pub enum OracleError {
    #[msg("Unauthorized")]
    Unauthorized,
}
