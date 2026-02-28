mod state;

use crate::state::UpdateData;
use anchor_lang::prelude::borsh::{BorshSchema, BorshSerialize};
use anchor_lang::prelude::*;
use anchor_lang::require_keys_eq;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use anchor_lang::solana_program::program::invoke_signed;
use anchor_lang::solana_program::{system_instruction, system_program};
use core::mem::size_of;
use ephemeral_rollups_sdk::anchor::{commit, ephemeral};
use ephemeral_rollups_sdk::ephem::commit_and_undelegate_accounts;
use ephemeral_rollups_sdk::types::DelegateAccountArgs;
use ephemeral_rollups_sdk::utils::{
    close_pda, close_pda_with_system_transfer, create_pda, seeds_with_bump,
};
use pyth_solana_receiver_sdk::price_update::{PriceFeedMessage, PriceUpdateV2, VerificationLevel};

declare_id!("PriCems5tHihc6UDXDjzjeawomAwBduWMGAi8ZUjppd");

#[cfg(not(feature = "test-mode"))]
const ORACLE_IDENTITY: Pubkey = pubkey!("MPUxHCpNUy3K1CSVhebAmTbcTCKVxfk9YMDcUP2ZnEA");
const SEED_PREFIX: &[u8] = b"price_feed";
const DELEGATE_WITH_ANY_VALIDATOR_DISCRIMINATOR: u8 = 19;

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

        Ok(())
    }

    pub fn delegate_price_feed(
        ctx: Context<DelegatePriceFeed>,
        provider: String,
        symbol: String,
    ) -> Result<()> {
        ensure_oracle(&ctx.accounts.payer)?;

        let pda_seeds: &[&[u8]] = &[SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()];
        let price_feed_key = ctx.accounts.price_feed.key();
        let buffer_seeds: &[&[u8]] = &[b"buffer", price_feed_key.as_ref()];

        let (_, delegate_account_bump) =
            Pubkey::find_program_address(pda_seeds, ctx.accounts.owner_program.key);
        let (_, buffer_pda_bump) =
            Pubkey::find_program_address(buffer_seeds, ctx.accounts.owner_program.key);

        let delegate_account_bump_slice: [u8; 1] = [delegate_account_bump];
        let pda_signer_seed_vec = seeds_with_bump(pda_seeds, &delegate_account_bump_slice);
        let pda_signer_seeds: &[&[&[u8]]] = &[&pda_signer_seed_vec];

        let buffer_bump_slice: [u8; 1] = [buffer_pda_bump];
        let buffer_signer_seed_vec = seeds_with_bump(buffer_seeds, &buffer_bump_slice);
        let buffer_signer_seeds: &[&[&[u8]]] = &[&buffer_signer_seed_vec];

        let payer_info = ctx.accounts.payer.to_account_info();
        let system_program_info = ctx.accounts.system_program.to_account_info();
        let data_len = ctx.accounts.price_feed.data_len();

        create_pda(
            &ctx.accounts.buffer_price_feed,
            ctx.accounts.owner_program.key,
            data_len,
            buffer_signer_seeds,
            &system_program_info,
            &payer_info,
        )?;

        {
            let pda_ro = ctx.accounts.price_feed.try_borrow_data()?;
            let mut buf = ctx.accounts.buffer_price_feed.try_borrow_mut_data()?;
            buf.copy_from_slice(&pda_ro);
        }

        {
            let mut pda_mut = ctx.accounts.price_feed.try_borrow_mut_data()?;
            pda_mut.fill(0);
        }

        if ctx.accounts.price_feed.owner != system_program_info.key {
            ctx.accounts.price_feed.assign(system_program_info.key);
        }

        if ctx.accounts.price_feed.owner != ctx.accounts.delegation_program.key {
            invoke_signed(
                &system_instruction::assign(
                    ctx.accounts.price_feed.key,
                    ctx.accounts.delegation_program.key,
                ),
                &[ctx.accounts.price_feed.clone(), system_program_info.clone()],
                pda_signer_seeds,
            )?;
        }

        let delegate_args = DelegateAccountArgs {
            commit_frequency_ms: u32::MAX,
            seeds: pda_seeds.iter().map(|seed| seed.to_vec()).collect(),
            validator: Some(system_program::id()),
        };

        let mut data = (DELEGATE_WITH_ANY_VALIDATOR_DISCRIMINATOR as u64)
            .to_le_bytes()
            .to_vec();
        delegate_args
            .serialize(&mut data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;

        let delegation_instruction = Instruction {
            program_id: ctx.accounts.delegation_program.key(),
            accounts: vec![
                AccountMeta::new(ctx.accounts.payer.key(), true),
                AccountMeta::new(price_feed_key, true),
                AccountMeta::new_readonly(ctx.accounts.owner_program.key(), false),
                AccountMeta::new(ctx.accounts.buffer_price_feed.key(), false),
                AccountMeta::new(ctx.accounts.delegation_record_price_feed.key(), false),
                AccountMeta::new(ctx.accounts.delegation_metadata_price_feed.key(), false),
                AccountMeta::new_readonly(system_program::id(), false),
            ],
            data,
        };

        invoke_signed(
            &delegation_instruction,
            &[
                payer_info.clone(),
                ctx.accounts.price_feed.clone(),
                ctx.accounts.owner_program.clone(),
                ctx.accounts.buffer_price_feed.clone(),
                ctx.accounts.delegation_record_price_feed.clone(),
                ctx.accounts.delegation_metadata_price_feed.clone(),
                system_program_info.clone(),
            ],
            pda_signer_seeds,
        )?;

        close_pda_with_system_transfer(
            &ctx.accounts.buffer_price_feed,
            buffer_signer_seeds,
            &payer_info,
            &system_program_info,
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

#[derive(Accounts)]
#[instruction(provider: String, symbol: String)]
pub struct DelegatePriceFeed<'info> {
    pub payer: Signer<'info>,
    /// CHECK: delegated PDA
    #[account(
        mut,
        seeds = [SEED_PREFIX, provider.as_bytes(), symbol.as_bytes()],
        bump
    )]
    pub price_feed: AccountInfo<'info>,
    /// CHECK: The buffer account
    #[account(
        mut,
        seeds = [b"buffer", price_feed.key().as_ref()],
        bump,
        seeds::program = crate::id()
    )]
    pub buffer_price_feed: AccountInfo<'info>,
    /// CHECK: The delegation record account
    #[account(
        mut,
        seeds = [b"delegation", price_feed.key().as_ref()],
        bump,
        seeds::program = delegation_program.key()
    )]
    pub delegation_record_price_feed: AccountInfo<'info>,
    /// CHECK: The delegation metadata account
    #[account(
        mut,
        seeds = [b"delegation-metadata", price_feed.key().as_ref()],
        bump,
        seeds::program = delegation_program.key()
    )]
    pub delegation_metadata_price_feed: AccountInfo<'info>,
    /// CHECK: The owner program of the delegated account PDA.
    #[account(address = crate::id())]
    pub owner_program: AccountInfo<'info>,
    /// CHECK: The delegation program.
    #[account(address = ephemeral_rollups_sdk::id())]
    pub delegation_program: AccountInfo<'info>,
    pub system_program: Program<'info, System>,
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
