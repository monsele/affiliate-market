use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, MintTo};
use mpl_token_metadata::instruction as mpl_instruction;
use solana_program::program::invoke;

declare_id!("ReplaceWithYourProgramPubkey");

#[program]
pub mod affiliate_nft {
    use super::*;

    pub fn create_campaign(
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
        campaign.bump = *ctx.bumps.get("campaign").unwrap();
        Ok(())
    }

    pub fn process_mint(ctx: Context<ProcessMint>, affiliate: Pubkey) -> Result<()> {
        let campaign = &ctx.accounts.campaign;

        // ---- Payments ----
        let total_price = campaign.price;
        let affiliate_cut = total_price * campaign.affiliate_fee_bps as u64 / 10_000;
        let creator_cut = total_price.checked_sub(affiliate_cut).unwrap();

        // Pay affiliate
        if affiliate_cut > 0 {
            invoke(
                &solana_program::system_instruction::transfer(
                    &ctx.accounts.buyer.key(),
                    &ctx.accounts.affiliate_receiver.key(),
                    affiliate_cut,
                ),
                &[
                    ctx.accounts.buyer.to_account_info(),
                    ctx.accounts.affiliate_receiver.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        // Pay creator
        if creator_cut > 0 {
            invoke(
                &solana_program::system_instruction::transfer(
                    &ctx.accounts.buyer.key(),
                    &ctx.accounts.creator.key(),
                    creator_cut,
                ),
                &[
                    ctx.accounts.buyer.to_account_info(),
                    ctx.accounts.creator.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        // ---- Mint NFT ----
        let cpi_accounts = MintTo {
            mint: ctx.accounts.nft_mint.to_account_info(),
            to: ctx.accounts.buyer_nft_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };

        let bump = *ctx.bumps.get("mint_authority").unwrap();
        let seeds = &[b"mint_auth", campaign.key().as_ref(), &[bump]];
        let signer_seeds = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::mint_to(cpi_ctx, 1)?;

        // ---- Create Metadata ----
        let metadata_ix = mpl_instruction::create_metadata_accounts_v3(
            ctx.accounts.token_metadata_program.key(),
            ctx.accounts.metadata.key(),
            ctx.accounts.nft_mint.key(),
            ctx.accounts.mint_authority.key(),
            ctx.accounts.buyer.key(),
            ctx.accounts.mint_authority.key(),
            campaign.name.clone(),
            campaign.symbol.clone(),
            campaign.uri.clone(),
            Some(vec![mpl_token_metadata::state::Creator {
                address: ctx.accounts.creator.key(),
                verified: false,
                share: 100,
            }]),
            0,
            true,
            false,
            None,
            None,
            None,
        );

        invoke(
            &metadata_ix,
            &[
                ctx.accounts.metadata.to_account_info(),
                ctx.accounts.nft_mint.to_account_info(),
                ctx.accounts.mint_authority.to_account_info(),
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.token_metadata_program.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
                ctx.accounts.rent.to_account_info(),
            ],
        )?;

        // ---- Verify into Collection ----
        let verify_ix = mpl_instruction::verify_sized_collection_item(
            ctx.accounts.token_metadata_program.key(),
            ctx.accounts.metadata.key(),
            ctx.accounts.mint_authority.key(),
            ctx.accounts.collection_mint.key(),
            ctx.accounts.collection_metadata.key(),
            ctx.accounts.collection_master_edition.key(),
            None,
        );

        invoke(
            &verify_ix,
            &[
                ctx.accounts.metadata.to_account_info(),
                ctx.accounts.mint_authority.to_account_info(),
                ctx.accounts.collection_mint.to_account_info(),
                ctx.accounts.collection_metadata.to_account_info(),
                ctx.accounts.collection_master_edition.to_account_info(),
                ctx.accounts.token_metadata_program.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
                ctx.accounts.rent.to_account_info(),
            ],
        )?;

        Ok(())
    }
}

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

#[account]
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
