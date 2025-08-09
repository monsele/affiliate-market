
use anchor_lang::prelude::*;

use crate::state::*;

#[derive(Accounts)]
#[instruction(price: u64, affiliate_fee_bps: u16, name: String, symbol: String, uri: String)]
pub struct CreateCampaign<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = 8 + Campaign::MAX_SIZE,
        seeds = [b"campaign", collection_mint.key().as_ref()],
        bump
    )]
    pub campaign: Account<'info, Campaign>,

    /// CHECK: This is the collection mint for NFTs in this campaign
    pub collection_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}


pub fn create_nft_campaign_instruction(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16, // 100 bps = 1%
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;
        campaign.creator = ctx.accounts.creator.key();
        campaign.price = price;
        campaign.affiliate_fee_bps = affiliate_fee_bps;
        campaign.collection_mint = ctx.accounts.collection_mint.key();
        campaign.name = name;
        campaign.symbol = symbol;
        campaign.uri = uri;
        campaign.bump = ctx.bumps.campaign;
        msg!("Campaign created with price: {}, affiliate fee: {} bps, name: {}, symbol: {}, uri: {}", 
            price, affiliate_fee_bps, &campaign.name, &campaign.symbol, &campaign.uri);
        Ok(())
    }