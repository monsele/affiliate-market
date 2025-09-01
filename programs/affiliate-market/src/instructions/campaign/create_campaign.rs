use anchor_lang::prelude::*;

use crate::state::Campaign;
use crate::error::ErrorCode;

#[derive(Accounts)]
#[instruction(price: u64, affiliate_fee_bps: u16, max_supply: u64)]
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
    pub campaign: Box<Account<'info, Campaign>>,

    /// CHECK: Collection mint created externally by creator
    pub collection_mint: UncheckedAccount<'info>,

    /// CHECK: Collection authority PDA - program-controlled authority
    #[account(
        mut,
        seeds = [b"collection_auth", campaign.key().as_ref()],
        bump
    )]
    pub collection_authority: UncheckedAccount<'info>,

    /// CHECK: Mint authority PDA - program-controlled authority  
    #[account(
        mut,
        seeds = [b"mint_auth", campaign.key().as_ref()],
        bump
    )]
    pub mint_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

 /// Creator creates campaign, storing collection_mint and bump info.
    pub fn create_campaign_instruction(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16,
        max_supply: u64,
    ) -> Result<()> {
        require!(affiliate_fee_bps <= 10000, ErrorCode::InvalidFee);

        let campaign = &mut ctx.accounts.campaign;
        campaign.creator = ctx.accounts.creator.key();
        campaign.collection_mint = ctx.accounts.collection_mint.key();
        campaign.price = price;
        campaign.affiliate_fee_bps = affiliate_fee_bps;
        campaign.minted = 0;
        campaign.max_supply = max_supply;

        // store bumps from ctx.bumps (dot access)
        campaign.mint_authority_bump = ctx.bumps.mint_authority;
        campaign.collection_auth_bump = ctx.bumps.collection_authority;

        Ok(())
    }
