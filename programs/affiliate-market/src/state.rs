use anchor_lang::prelude::*;

#[account]
#[derive(Debug)]
pub struct Campaign {
    pub creator: Pubkey,
    pub price: u64,
    pub affiliate_fee_bps: u16,
    pub collection_mint: Pubkey,
    pub name: String,
    pub symbol: String,
    pub uri: String,
    pub bump: u8,
}
impl Campaign {
    pub const MAX_SIZE: usize =
        32 + 8 + 2 + 32 + (4 + 32) + (4 + 10) + (4 + 200) + 1;
}