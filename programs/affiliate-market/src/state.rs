use anchor_lang::prelude::*;

// #[account]
// #[derive(Debug)]
// pub struct Campaign {
//     pub creator: Pubkey,
//     pub price: u64,
//     pub affiliate_fee_bps: u16,
//     pub collection_mint: Pubkey,
//     pub name: String,
//     pub symbol: String,
//     pub uri: String,
//     pub bump: u8,
// }
// impl Campaign {
//     pub const MAX_SIZE: usize =
//         32 + 8 + 2 + 32 + (4 + 32) + (4 + 10) + (4 + 200) + 1;
// }


#[account]
#[derive(Debug)]
pub struct Campaign {
    pub creator: Pubkey,
    pub collection_mint: Pubkey,
    pub price: u64,
    pub affiliate_fee_bps: u16,
    pub minted: u64,
    pub max_supply: u64,
    pub mint_authority_bump: u8,
    pub collection_auth_bump: u8,
}
impl Campaign {
    pub const SIZE: usize = 32 + 32 + 8 + 2 + 8 + 8 + 1 + 1;
}