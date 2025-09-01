use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, MintTo, InitializeMint},
};
use mpl_token_metadata::instructions::{
    CreateMetadataAccountV3Cpi, CreateMetadataAccountV3CpiAccounts, CreateMetadataAccountV3InstructionArgs,
    CreateMasterEditionV3Cpi, CreateMasterEditionV3CpiAccounts, CreateMasterEditionV3InstructionArgs,
    // If your mpl version exposes VerifySizedCollectionItemCpi builder, import it similarly:
    VerifySizedCollectionItemCpi, VerifySizedCollectionItemCpiAccounts,
};
use mpl_token_metadata::types::DataV2;
use mpl_token_metadata::ID as MPL_TOKEN_METADATA_ID;
use anchor_lang::{ solana_program::{program::invoke_signed, program::invoke, system_instruction}};

 use spl_token::solana_program::program_pack::Pack;

//use solana_program::{program::invoke_signed, program::invoke, system_instruction};
use spl_token::state::Mint as SplMint;

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

    /// Buyer mints an NFT for a campaign. affiliate_maybe: Optional affiliate pubkey.
    /// This instruction:
    ///  - collects payment (buyer -> creator & affiliate)
    ///  - creates mint PDA for the NFT and initializes it
    ///  - creates buyer ATA if needed
    ///  - mints 1 token to buyer ATA (signed by mint_authority PDA)
    ///  - creates metadata & master edition via Metaplex CPI
    ///  - verifies the minted NFT into the collection via collection_authority PDA
    ///  - updates affiliate stats PDA
   #[access_control(validate_accounts(&ctx))]
    pub fn process_mint(ctx: Context<ProcessMint>, affiliate_maybe: Option<Pubkey>, name: String, symbol: String, uri: String) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;

        // 1) supply check
        require!(campaign.minted < campaign.max_supply, ErrorCode::SoldOut);

        // 2) payment calculation & transfers via system_instruction::transfer + invoke
        let price = campaign.price;
        let affiliate_cut = ((price as u128) * (campaign.affiliate_fee_bps as u128) / 10_000u128) as u64;
        let creator_cut = price.checked_sub(affiliate_cut).ok_or(ErrorCode::MathOverflow)?;

        // transfer creator_cut
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

        // transfer affiliate_cut if provided
        if let Some(affiliate_pk) = affiliate_maybe {
            if affiliate_cut > 0 {
                invoke(
                    &system_instruction::transfer(&ctx.accounts.buyer.key(), &affiliate_pk, affiliate_cut),
                    &[
                        ctx.accounts.buyer.to_account_info(),
                        ctx.accounts.affiliate_receiver.to_account_info(),
                        ctx.accounts.system_program.to_account_info(),
                    ],
                )?;
            }
        } else {
            // optional: if no affiliate, send affiliate_cut to creator
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

        // 3) derive expected mint PDA using campaign key and current minted index
        let index_bytes = campaign.minted.to_le_bytes();
        let mint_seeds: &[&[u8]] = &[b"nft_mint", campaign.to_account_info().key.as_ref(), &index_bytes];
        let (derived_mint_pda, derived_mint_bump) = Pubkey::find_program_address(mint_seeds, ctx.program_id);
        require!(derived_mint_pda == ctx.accounts.nft_mint.key(), ErrorCode::InvalidMintAccount);

        // 4) create account for mint PDA (owner = spl_token::id()) using buyer as payer (buyer included)
        let rent = Rent::get()?;
        let mint_rent = rent.minimum_balance(SplMint::LEN);
        let create_account_ix = system_instruction::create_account(
            &ctx.accounts.buyer.key(),
            &derived_mint_pda,
            mint_rent,
            SplMint::LEN as u64,
            &spl_token::id(),
        );

        invoke_signed(
            &create_account_ix,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.nft_mint.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[&[b"nft_mint", campaign.key().as_ref(), &index_bytes, &[derived_mint_bump]]],
        )?;

        // 5) initialize mint with decimals=0 and mint authority = mint_auth PDA
        let (mint_auth_pda, _mint_auth_bump) = Pubkey::find_program_address(&[b"mint_auth", campaign.key().as_ref()], ctx.program_id);
        let cpi_init_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::InitializeMint {
                mint: ctx.accounts.nft_mint.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
        );
        // Note: anchor_spl::token::initialize_mint signature: initialize_mint(ctx, decimals, mint_authority, freeze_authority)
        token::initialize_mint(cpi_init_ctx, 0, &mint_auth_pda, Some(&mint_auth_pda))?;

        // 6) buyer_ata: `init_if_needed` in accounts ensures it exists and buyer pays rent
        // (handled by Anchor via the attribute on the account)

        // 7) mint_to buyer_ata using mint_authority PDA (signed by program via seeds)
        let mint_auth_bump = campaign.mint_authority_bump;
        let binding = campaign.key();
        let seeds_for_mint_auth: &[&[u8]] = &[b"mint_auth", binding.as_ref(), &[mint_auth_bump]];
        let signer_seeds_mint_auth = &[seeds_for_mint_auth];

        let cpi_accounts_mint_to = token::MintTo {
            mint: ctx.accounts.nft_mint.to_account_info(),
            to: ctx.accounts.buyer_ata.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_program_mint = ctx.accounts.token_program.to_account_info();
        let cpi_ctx_mint = CpiContext::new_with_signer(cpi_program_mint, cpi_accounts_mint_to, signer_seeds_mint_auth);
        token::mint_to(cpi_ctx_mint, 1)?;

        // 8) validate metadata & edition PDAs match canonical derivation (prevent fake accounts)
        let (derived_metadata_pda, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                MPL_TOKEN_METADATA_ID.as_ref(),
                ctx.accounts.nft_mint.key().as_ref(),
            ],
            &MPL_TOKEN_METADATA_ID,
        );
        require!(derived_metadata_pda == ctx.accounts.metadata.key(), ErrorCode::InvalidMetadata);

        let (derived_edition_pda, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                MPL_TOKEN_METADATA_ID.as_ref(),
                ctx.accounts.nft_mint.key().as_ref(),
                b"edition",
            ],
            &MPL_TOKEN_METADATA_ID,
        );
        require!(derived_edition_pda == ctx.accounts.master_edition.key(), ErrorCode::InvalidMasterEdition);

        // 9) create metadata via Metaplex CPI (buyer pays)
        let data_v2 = DataV2 {
            name: name.clone(),
            symbol: symbol.clone(),
            uri: uri.clone(),
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        };

        // Build CPI call using CreateMetadataAccountV3Cpi builder
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

        // 10) create master edition via CPI
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

        // 11) verify minted item into collection: use collection authority PDA as signer
        // compute and validate collection metadata PDA as well
        let (derived_collection_metadata, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                MPL_TOKEN_METADATA_ID.as_ref(),
                ctx.accounts.collection_mint.key().as_ref(),
            ],
            &MPL_TOKEN_METADATA_ID,
        );
        require!(derived_collection_metadata == ctx.accounts.collection_metadata.key(), ErrorCode::InvalidCollectionMetadata);

        // collection authority bump and seeds
        let coll_auth_bump = campaign.collection_auth_bump;
        let coll_auth_seeds: &[&[u8]] = &[b"collection_auth", campaign.to_account_info().key.as_ref(), &[coll_auth_bump]];
        let signer_seeds_collection_auth = &[coll_auth_seeds];

        // Build verify CPI (builder) and invoke_signed with collection authority seeds
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

        // 12) update affiliate stats PDA if provided
        if let Some(affiliate_pk) = affiliate_maybe {
            let stats = &mut ctx.accounts.affiliate_stats;
            stats.total_mints = stats.total_mints.checked_add(1).ok_or(ErrorCode::MathOverflow)?;
            stats.total_earned = stats.total_earned.checked_add(affiliate_cut).ok_or(ErrorCode::MathOverflow)?;
        }

        // 13) increment campaign minted
        campaign.minted = campaign.minted.checked_add(1).ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }
}

/// Accounts

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

    /// collection mint (created earlier by creator via Metaplex JS)
    pub collection_mint: UncheckedAccount<'info>,

    /// collection authority PDA (program-owned PDA that the creator delegates as collection authority)
    /// This account will be used when calling verify_sized_collection_item — creator must delegate authority to this PDA off-chain.
    /// We store it here as an account to ensure it's present in transactions. Not initialized here — program expects creator to delegate separately.
    #[account(mut,
     seeds = [b"collection_auth", campaign.key().as_ref()],
     bump
    )]
    pub collection_authority: UncheckedAccount<'info>,

    /// mint authority PDA (derived in program with seeds ["mint_auth", campaign.key()])
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
pub struct ProcessMint<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    #[account(mut, has_one = collection_mint)]
    pub campaign: Account<'info, Campaign>,

    /// recipient = campaign.creator
    #[account(mut, address = campaign.creator)]
    pub creator: UncheckedAccount<'info>,

    /// affiliate receiver (used as destination in transfer)
    #[account(mut)]
    pub affiliate_receiver: UncheckedAccount<'info>,

    /// the NFT mint PDA (created by program during this instruction; frontend must compute and pass the PDA)
    #[account(mut)]
    pub nft_mint: UncheckedAccount<'info>,

    /// buyer's ATA for nft_mint (init_if_needed, payer = buyer)
    #[account(
        init_if_needed,
       // mut,
        associated_token::mint = nft_mint,
        associated_token::authority = buyer,
        payer = buyer
    )]
    pub buyer_ata: Account<'info, TokenAccount>,

    /// mint authority PDA (seeds ["mint_auth", campaign.key()])
    #[account(mut)]
    pub mint_authority: UncheckedAccount<'info>,

    /// metadata PDA for minted NFT (must equal canonical metadata PDA)
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// master edition PDA for minted NFT (must equal canonical edition PDA)
    #[account(mut)]
    pub master_edition: UncheckedAccount<'info>,

    /// collection mint & its metadata/master edition (already created by creator)
    pub collection_mint: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection_metadata: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection_master_edition: UncheckedAccount<'info>,

    /// collection authority PDA (seeds ["collection_auth", campaign.key()])
    #[account(mut)]
    pub collection_authority: UncheckedAccount<'info>,

    /// affiliate stats PDA (init_if_needed, payer = buyer)
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + AffiliateStats::SIZE,
        seeds = [b"affiliate", campaign.key().as_ref(), affiliate_receiver.key.as_ref()],
        bump
    )]
    pub affiliate_stats: Account<'info, AffiliateStats>,

    /// programs
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_metadata_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Accounts structures

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
    pub const SIZE: usize = 32 + 32 + 8 + 2 + 8 + 8 + 1 + 1;
}

#[account]
pub struct AffiliateStats {
    pub total_mints: u64,
    pub total_earned: u64,
}
impl AffiliateStats {
    pub const SIZE: usize = 8 + 8;
}

/// validation helper: ensure metadata / edition PDAs are canonical
fn validate_accounts(ctx: &Context<ProcessMint>) -> Result<()> {
    let (derived_metadata_pda, _) = Pubkey::find_program_address(
        &[b"metadata", MPL_TOKEN_METADATA_ID.as_ref(), ctx.accounts.nft_mint.key().as_ref()],
        &MPL_TOKEN_METADATA_ID,
    );
    require!(derived_metadata_pda == ctx.accounts.metadata.key(), ErrorCode::InvalidMetadata);

    let (derived_edition_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            MPL_TOKEN_METADATA_ID.as_ref(),
            ctx.accounts.nft_mint.key().as_ref(),
            b"edition",
        ],
        &MPL_TOKEN_METADATA_ID,
    );
    require!(derived_edition_pda == ctx.accounts.master_edition.key(), ErrorCode::InvalidMasterEdition);

    Ok(())
}

/// Errors
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
