use anchor_lang::prelude::*;



#[account]
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
    pub const SIZE: usize = 32 + 32 + 8 + 2 + 8 + 8 + 1 + 1; // 92 bytes
}

#[account]
pub struct AffiliateStats {
    pub total_mints: u64,
    pub total_earned: u64,
}

impl AffiliateStats {
    pub const SIZE: usize = 8 + 8; // 16 bytes
}
