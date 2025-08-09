use anchor_lang::prelude::*;
mod instructions;
mod state;
use instructions::*;
mod error;
declare_id!("6jxp4eoRZ8C7qVeXKyHk68YEmCoBVHR1AQxJ9Le4Aey1");

#[program]
pub mod affiliate_market {
    pub use super::*;
   pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16, // 100 bps = 1%
        name: String,
        symbol: String,
        uri: String) -> Result<()>  {

     create_nft_campaign_instruction(
            ctx,
            price,
            affiliate_fee_bps,
            name,
            symbol,
            uri,
        )
       //Ok(())
    }
}


