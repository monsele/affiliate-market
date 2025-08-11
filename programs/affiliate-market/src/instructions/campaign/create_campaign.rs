
use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
#[instruction(price: u64, affiliate_fee_bps: u16)]
pub struct CreateCampaign<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + Campaign::SIZE,
        seeds = [b"campaign", collection_mint.key().as_ref()],
        bump
    )]
    pub campaign: Account<'info, Campaign>,

    /// collection mint (created previously by creator via Metaplex JS)
    pub collection_mint: UncheckedAccount<'info>,

    /// PDA used as program's collection authority (delegated by creator once off-chain)
    /// This account is a PDA derived as ["collection_auth", campaign.key()]
    /// It will be used to sign verify_collection CPI.
    /// Initialized by program via bump storage only — creator must call Metaplex to delegate authority.
    #[account(mut)]
    pub collection_authority: UncheckedAccount<'info>,

    /// PDA to be used as mint authority for minted NFTs
    /// Seeds: ["mint_auth", campaign.key()]
    /// Not stored as an account here, stored bump in campaign.
    #[account(mut)]
    pub mint_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

 pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16,
    ) -> Result<()> {
        require!(affiliate_fee_bps <= 10000, ErrorCode::InvalidFee);

        let campaign = &mut ctx.accounts.campaign;
        campaign.creator = ctx.accounts.creator.key();
        campaign.collection_mint = ctx.accounts.collection_mint.key();
        campaign.price = price;
        campaign.affiliate_fee_bps = affiliate_fee_bps;
        campaign.minted = 0;

        // store PDA bumps for later use
        campaign.mint_authority_bump = *ctx.bumps.get("mint_authority").unwrap();
        campaign.collection_auth_bump = *ctx.bumps.get("collection_authority").unwrap();

        Ok(())
    }