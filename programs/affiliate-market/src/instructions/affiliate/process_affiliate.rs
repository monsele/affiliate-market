
#[derive(Accounts)]
pub struct ProcessMint<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(mut)]
    pub campaign: Account<'info, Campaign>,

    /// CHECK: Verified in program
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,

    /// CHECK: Verified in program
    #[account(mut)]
    pub affiliate_receiver: UncheckedAccount<'info>,

    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,

    #[account(mut)]
    pub buyer_nft_account: Account<'info, TokenAccount>,

    /// PDA mint authority for campaign
    /// Seeds: ["mint_auth", campaign.key()]
    pub mint_authority: UncheckedAccount<'info>,

    /// CHECK: Metadata account
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// CHECK: Collection mint
    pub collection_mint: UncheckedAccount<'info>,

    /// CHECK: Metadata of the collection
    #[account(mut)]
    pub collection_metadata: UncheckedAccount<'info>,

    /// CHECK: Master edition of the collection
    #[account(mut)]
    pub collection_master_edition: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
    /// CHECK: Metaplex program
    pub token_metadata_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}



