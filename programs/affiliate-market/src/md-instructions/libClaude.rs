use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, MintTo},
};

use mpl_token_metadata::instructions::{
    CreateMetadataAccountV3Cpi, CreateMetadataAccountV3CpiAccounts, CreateMetadataAccountV3InstructionArgs,
    CreateMasterEditionV3Cpi, CreateMasterEditionV3CpiAccounts, CreateMasterEditionV3InstructionArgs,
    VerifySizedCollectionItemCpi, VerifySizedCollectionItemCpiAccounts,
};
use mpl_token_metadata::types::DataV2;
use mpl_token_metadata::ID as MPL_TOKEN_METADATA_ID;
use anchor_lang::solana_program::{program::invoke_signed, program::invoke, system_instruction};

declare_id!("6jxp4eoRZ8C7qVeXKyHk68YEmCoBVHR1AQxJ9Le4Aey1");

#[program]
pub mod secure_affiliate_candy {
    use super::*;

    /// Creator creates campaign, storing collection_mint and bump info.
    pub fn create_campaign(
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

    /// Buyer mints an NFT for a campaign with optional affiliate
    pub fn process_mint(
        ctx: Context<ProcessMint>, 
        affiliate_maybe: Option<Pubkey>, 
        name: String, 
        symbol: String, 
        uri: String
    ) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;

        // 1) Supply check
        require!(campaign.minted < campaign.max_supply, ErrorCode::SoldOut);

        // 2) Payment calculation & transfers
        let price = campaign.price;
        let affiliate_cut = ((price as u128) * (campaign.affiliate_fee_bps as u128) / 10_000u128) as u64;
        let creator_cut = price.checked_sub(affiliate_cut).ok_or(ErrorCode::MathOverflow)?;

        // Transfer creator_cut
        if creator_cut > 0 {
            invoke(
                &system_instruction::transfer(&ctx.accounts.buyer.key(), &ctx.accounts.creator.key(), creator_cut),
                &[
                    ctx.accounts.buyer.to_account_info(),
                    ctx.accounts.creator.to_account_info(),
                    ctx.accounts.system_program.to_account_info(),
                ],
            )?;
        }

        // Transfer affiliate_cut if provided
        if let Some(_affiliate_pk) = affiliate_maybe {
            if affiliate_cut > 0 && ctx.accounts.affiliate_receiver.key() != Pubkey::default() {
                invoke(
                    &system_instruction::transfer(&ctx.accounts.buyer.key(), &ctx.accounts.affiliate_receiver.key(), affiliate_cut),
                    &[
                        ctx.accounts.buyer.to_account_info(),
                        ctx.accounts.affiliate_receiver.to_account_info(),
                        ctx.accounts.system_program.to_account_info(),
                    ],
                )?;
            }
        } else {
            // If no affiliate, send affiliate_cut to creator
            if affiliate_cut > 0 {
                invoke(
                    &system_instruction::transfer(&ctx.accounts.buyer.key(), &ctx.accounts.creator.key(), affiliate_cut),
                    &[
                        ctx.accounts.buyer.to_account_info(),
                        ctx.accounts.creator.to_account_info(),
                        ctx.accounts.system_program.to_account_info(),
                    ],
                )?;
            }
        }

        // 3) Use Anchor's built-in mint initialization instead of manual creation
        // The mint is already initialized via the account constraints

        // 4) Mint token to buyer
        let mint_auth_bump = campaign.mint_authority_bump;
        let binding = campaign.key();
        let seeds_for_mint_auth: &[&[u8]] = &[b"mint_auth", binding.as_ref(), &[mint_auth_bump]];
        let signer_seeds_mint_auth = &[seeds_for_mint_auth];

        let cpi_accounts_mint_to = MintTo {
            mint: ctx.accounts.nft_mint.to_account_info(),
            to: ctx.accounts.buyer_ata.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_program_mint = ctx.accounts.token_program.to_account_info();
        let cpi_ctx_mint = CpiContext::new_with_signer(cpi_program_mint, cpi_accounts_mint_to, signer_seeds_mint_auth);
        token::mint_to(cpi_ctx_mint, 1)?;

        // 5) Create metadata via Metaplex CPI
        let data_v2 = DataV2 {
            name: name.clone(),
            symbol: symbol.clone(),
            uri: uri.clone(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        CreateMetadataAccountV3Cpi::new(
            &ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountV3CpiAccounts {
                metadata: &ctx.accounts.metadata.to_account_info(),
                mint: &ctx.accounts.nft_mint.to_account_info(),
                mint_authority: &ctx.accounts.mint_authority.to_account_info(),
                payer: &ctx.accounts.buyer.to_account_info(),
                update_authority: (&ctx.accounts.mint_authority.to_account_info(), true),
                system_program: &ctx.accounts.system_program.to_account_info(),
                rent: Some(&ctx.accounts.rent.to_account_info()),
            },
            CreateMetadataAccountV3InstructionArgs {
                data: data_v2,
                is_mutable: true,
                collection_details: None,
            },
        )
        .invoke_signed(&[&[b"mint_auth", campaign.key().as_ref(), &[mint_auth_bump]]])?;

        // 6) Create master edition
        CreateMasterEditionV3Cpi::new(
            &ctx.accounts.token_metadata_program.to_account_info(),
            CreateMasterEditionV3CpiAccounts {
                edition: &ctx.accounts.master_edition.to_account_info(),
                mint: &ctx.accounts.nft_mint.to_account_info(),
                update_authority: &ctx.accounts.mint_authority.to_account_info(),
                mint_authority: &ctx.accounts.mint_authority.to_account_info(),
                payer: &ctx.accounts.buyer.to_account_info(),
                metadata: &ctx.accounts.metadata.to_account_info(),
                token_program: &ctx.accounts.token_program.to_account_info(),
                system_program: &ctx.accounts.system_program.to_account_info(),
                rent: Some(&ctx.accounts.rent.to_account_info()),
            },
            CreateMasterEditionV3InstructionArgs { max_supply: Some(0) },
        )
        .invoke_signed(&[&[b"mint_auth", campaign.key().as_ref(), &[mint_auth_bump]]])?;

        // 7) Verify minted item into collection
        let coll_auth_bump = campaign.collection_auth_bump;
        let coll_auth_seeds: &[&[u8]] = &[b"collection_auth", campaign.to_account_info().key.as_ref(), &[coll_auth_bump]];
        let signer_seeds_collection_auth = &[coll_auth_seeds];

        VerifySizedCollectionItemCpi::new(
            &ctx.accounts.token_metadata_program.to_account_info(),
            VerifySizedCollectionItemCpiAccounts {
                metadata: &ctx.accounts.metadata.to_account_info(),
                collection_authority: &ctx.accounts.collection_authority.to_account_info(),
                payer: &ctx.accounts.buyer.to_account_info(),
                collection_mint: &ctx.accounts.collection_mint.to_account_info(),
                collection: &ctx.accounts.collection_metadata.to_account_info(),
                collection_master_edition_account: &ctx.accounts.collection_master_edition.to_account_info(),
                collection_authority_record: None,
            },
        )
        .invoke_signed(signer_seeds_collection_auth)?;

        // 8) Update affiliate stats if provided
        if let Some(_affiliate_pk) = affiliate_maybe {
            if ctx.accounts.affiliate_receiver.key() != Pubkey::default() {
                let stats = &mut ctx.accounts.affiliate_stats;
                stats.total_mints = stats.total_mints.checked_add(1).ok_or(ErrorCode::MathOverflow)?;
                stats.total_earned = stats.total_earned.checked_add(affiliate_cut).ok_or(ErrorCode::MathOverflow)?;
            }
        }

        // 9) Increment campaign minted count
        campaign.minted = campaign.minted.checked_add(1).ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }
}


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
    pub campaign: Account<'info, Campaign>,

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


#[derive(Accounts)]
#[instruction(affiliate_maybe: Option<Pubkey>, name: String, symbol: String, uri: String)]
pub struct ProcessMint<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(mut, has_one = collection_mint)]
    pub campaign: Account<'info, Campaign>,

    /// CHECK: Creator account verified via campaign.creator constraint
    #[account(mut, address = campaign.creator)]
    pub creator: UncheckedAccount<'info>,

    /// CHECK: Affiliate receiver account - can be any account
    #[account(mut)]
    pub affiliate_receiver: UncheckedAccount<'info>,

    /// NFT mint PDA - initialized by Anchor
    #[account(
        init,
        payer = buyer,
        mint::decimals = 0,
        mint::authority = mint_authority,
        seeds = [b"nft_mint", campaign.key().as_ref(), &campaign.minted.to_le_bytes()],
        bump
    )]
    pub nft_mint: Account<'info, Mint>,

    /// Buyer's associated token account
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = nft_mint,
        associated_token::authority = buyer
    )]
    pub buyer_ata: Account<'info, TokenAccount>,

    /// CHECK: Mint authority PDA - verified by seeds constraint
    #[account(
        seeds = [b"mint_auth", campaign.key().as_ref()],
        bump = campaign.mint_authority_bump
    )]
    pub mint_authority: UncheckedAccount<'info>,

    /// CHECK: Metadata account will be created by Metaplex CPI
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// CHECK: Master edition account will be created by Metaplex CPI
    #[account(mut)]
    pub master_edition: UncheckedAccount<'info>,

    /// CHECK: Collection mint - verified via campaign constraint
    pub collection_mint: UncheckedAccount<'info>,
    
    /// CHECK: Collection metadata account
    #[account(mut)]
    pub collection_metadata: UncheckedAccount<'info>,
    
    /// CHECK: Collection master edition account
    #[account(mut)]
    pub collection_master_edition: UncheckedAccount<'info>,

    /// CHECK: Collection authority PDA - verified by seeds constraint
    #[account(
        seeds = [b"collection_auth", campaign.key().as_ref()],
        bump = campaign.collection_auth_bump
    )]
    pub collection_authority: UncheckedAccount<'info>,

    /// Affiliate stats PDA
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + AffiliateStats::SIZE,
        seeds = [b"affiliate", campaign.key().as_ref(), affiliate_receiver.key().as_ref()],
        bump
    )]
    pub affiliate_stats: Account<'info, AffiliateStats>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// CHECK: Metaplex token metadata program
    pub token_metadata_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

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

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid affiliate fee (0..=10000 bps)")]
    InvalidFee,
    #[msg("Sold out")]
    SoldOut,
    #[msg("Math overflow")]
    MathOverflow,
    #[msg("Invalid mint account (PDA mismatch)")]
    InvalidMintAccount,
    #[msg("Invalid metadata PDA")]
    InvalidMetadata,
    #[msg("Invalid master edition PDA")]
    InvalidMasterEdition,
    #[msg("Invalid collection metadata")]
    InvalidCollectionMetadata,
}