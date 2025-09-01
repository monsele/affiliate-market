use anchor_lang::prelude::*;
mod instructions;
mod state;
mod error;
use instructions::*;
declare_id!("6jxp4eoRZ8C7qVeXKyHk68YEmCoBVHR1AQxJ9Le4Aey1");

#[program]
pub mod affiliate_market{
 use super::*;
   
pub fn process_mint(
        ctx: Context<ProcessMint>, 
        affiliate_maybe: Option<Pubkey>, 
        name: String, 
        symbol: String, 
        uri: String
    ) -> Result<()> {
       process_mint_instruction(ctx, affiliate_maybe, name, symbol, uri)
    }
     pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16,
        max_supply: u64,
    ) -> Result<()> {
        create_campaign_instruction(ctx, price, affiliate_fee_bps, max_supply)
    }
   
}