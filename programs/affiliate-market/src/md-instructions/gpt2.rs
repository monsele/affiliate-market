use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, MintTo, InitializeMint},
};
use mpl_token_metadata::instruction as mpl_instruction;
use solana_program::{program::invoke_signed, program::invoke, system_instruction};
use spl_token::state::Mint as SplMint;

declare_id!("ReplaceWithYourProgramIdHere");

#[program]
pub mod secure_affiliate_candy {
    use super::*;

    /// Creator creates campaign (collection already exists)
    pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        price: u64,
        affiliate_fee_bps: u16,
    ) -> Result<()> {
        require!(affiliate_fee_bps <= 10000, ErrorCode::InvalidFee);

        let campaign = &mut ctx.accounts.campaign;
        campaign.creator = ctx.accounts.creator.key();
        campaign.collection_mint = ctx.accounts.collection_mint.key();
        campaign.price = price;
        campaign.affiliate_fee_bps = affiliate_fee_bps;
        campaign.minted = 0;

        // store PDA bumps for later use
        campaign.mint_authority_bump = *ctx.bumps.get("mint_authority").unwrap();
        campaign.collection_auth_bump = *ctx.bumps.get("collection_authority").unwrap();

        Ok(())
    }

    /// Buyer mints an NFT. Program creates mint PDA, initializes mint, mints, creates metadata,
    /// verifies in collection (using collection authority PDA), updates affiliate stats.
    #[access_control(validate_accounts(&ctx))]
    pub fn process_mint(ctx: Context<ProcessMint>, affiliate_maybe: Option<Pubkey>) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;

        // 1) supply check
        require!(campaign.minted < campaign.max_supply, ErrorCode::SoldOut);

        let price = campaign.price;
        let affiliate_cut = ((price as u128) * (campaign.affiliate_fee_bps as u128) / 10_000u128) as u64;
        let creator_cut = price.checked_sub(affiliate_cut).ok_or(ErrorCode::MathOverflow)?;

        // 2) transfer lamports using system_instruction::transfer and invoke
        // transfer creator_cut to creator
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

        // transfer affiliate_cut to affiliate if provided
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
        } // else affiliate_cut already accounted for -> could be sent to creator as desired

        // 3) derive mint PDA for this NFT using campaign pubkey + current minted index
        let index_bytes = campaign.minted.to_le_bytes();
        let mint_seeds: &[&[u8]] = &[
            b"nft_mint",
            campaign.to_account_info().key.as_ref(),
            &index_bytes,
        ];
        let (mint_pda, mint_bump) = Pubkey::find_program_address(mint_seeds, ctx.program_id);

        require!(mint_pda == ctx.accounts.nft_mint.key(), ErrorCode::InvalidMintAccount);

        // 4) create account for mint_pda with owner = spl_token::id()
        let rent = Rent::get()?;
        let mint_rent = rent.minimum_balance(SplMint::LEN);
        let create_account_ix = system_instruction::create_account(
            &ctx.accounts.buyer.key(),
            &mint_pda,
            mint_rent,
            SplMint::LEN as u64,
            &spl_token::id(),
        );

        let signer_seeds: &[&[u8]] = &[b"nft_mint", campaign.to_account_info().key.as_ref(), &index_bytes, &[mint_bump]];
        invoke_signed(
            &create_account_ix,
            &[
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.nft_mint.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
            &[signer_seeds],
        )?;

        // 5) initialize mint (decimals=0) with mint_authority PDA as authority
        let (mint_auth, _) = Pubkey::find_program_address(&[b"mint_auth", campaign.to_account_info().key.as_ref()], ctx.program_id);
        let cpi_accounts = token::InitializeMint {
            mint: ctx.accounts.nft_mint.to_account_info(),
            rent: ctx.accounts.rent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::initialize_mint(cpi_ctx, 0, &mint_auth, Some(&mint_auth))?;

        // 6) create buyer ATA if needed (Anchor handles init_if_needed via account attr)
        // Anchor will create buyer_ata automatically if missing (attribute used in account definition)

        // 7) mint_to buyer_ata using mint_authority PDA as signer
        let mint_auth_bump = campaign.mint_authority_bump;
        let mint_auth_seeds: &[&[u8]] = &[b"mint_auth", campaign.to_account_info().key.as_ref(), &[mint_auth_bump]];
        let signer_seeds_mint_auth = &[mint_auth_seeds];
        let cpi_accounts_mint_to = token::MintTo {
            mint: ctx.accounts.nft_mint.to_account_info(),
            to: ctx.accounts.buyer_ata.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_ctx_mint_to = CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), cpi_accounts_mint_to, signer_seeds_mint_auth);
        token::mint_to(cpi_ctx_mint_to, 1)?;

        // 8) verify metadata PDA matches canonical derivation
        let (derived_metadata_pda, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                mpl_token_metadata::id().as_ref(),
                ctx.accounts.nft_mint.key().as_ref(),
            ],
            &mpl_token_metadata::id(),
        );
        require!(derived_metadata_pda == ctx.accounts.metadata.key(), ErrorCode::InvalidMetadata);

        // 9) create metadata via Metaplex CPI (payer = buyer)
        let creators = vec![mpl_token_metadata::state::Creator {
            address: ctx.accounts.creator.key(),
            verified: false,
            share: 100,
        }];

        let metadata_ix = mpl_instruction::create_metadata_accounts_v3(
            ctx.accounts.token_metadata_program.key(),
            ctx.accounts.metadata.key(),
            ctx.accounts.nft_mint.key(),
            ctx.accounts.mint_authority.key(),
            ctx.accounts.buyer.key(),
            ctx.accounts.mint_authority.key(),
            ctx.accounts.name.clone(),
            ctx.accounts.symbol.clone(),
            ctx.accounts.uri.clone(),
            Some(creators),
            0,
            true,
            false,
            None,
            None,
            None,
        );

        invoke_signed(
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
            &[signer_seeds_mint_auth],
        )?;

        // 10) create master edition via Metaplex CPI
        let master_edition_ix = mpl_instruction::create_master_edition_v3(
            ctx.accounts.token_metadata_program.key(),
            ctx.accounts.master_edition.key(),
            ctx.accounts.nft_mint.key(),
            ctx.accounts.mint_authority.key(),
            ctx.accounts.buyer.key(),
            ctx.accounts.metadata.key(),
            Some(0),
        );

        invoke_signed(
            &master_edition_ix,
            &[
                ctx.accounts.master_edition.to_account_info(),
                ctx.accounts.nft_mint.to_account_info(),
                ctx.accounts.mint_authority.to_account_info(),
                ctx.accounts.buyer.to_account_info(),
                ctx.accounts.metadata.to_account_info(),
                ctx.accounts.token_metadata_program.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
                ctx.accounts.rent.to_account_info(),
            ],
            &[signer_seeds_mint_auth],
        )?;

        // 11) verify collection via program's collection authority PDA (creator must have delegated)
        let collection_auth_bump = campaign.collection_auth_bump;
        let collection_auth_seeds: &[&[u8]] = &[b"collection_auth", campaign.to_account_info().key.as_ref(), &[collection_auth_bump]];
        let signer_seeds_collection_auth = &[collection_auth_seeds];

        // compute collection metadata PDA and master edition of collection and verify they match provided accounts
        let (derived_collection_metadata, _) = Pubkey::find_program_address(
            &[
                b"metadata",
                mpl_token_metadata::id().as_ref(),
                ctx.accounts.collection_mint.key().as_ref(),
            ],
            &mpl_token_metadata::id(),
        );
        require!(derived_collection_metadata == ctx.accounts.collection_metadata.key(), ErrorCode::InvalidCollectionMetadata);

        let verify_ix = mpl_instruction::verify_sized_collection_item(
            ctx.accounts.token_metadata_program.key(),
            ctx.accounts.metadata.key(),                 // metadata of minted item
            ctx.accounts.collection_authority.key(),     // collection_authority PDA (program)
            ctx.accounts.collection_mint.key(),          // collection mint
            ctx.accounts.collection_metadata.key(),      // collection metadata
            ctx.accounts.collection_master_edition.key(),// collection master edition
            None,
        );

        invoke_signed(
            &verify_ix,
            &[
                ctx.accounts.metadata.to_account_info(),
                ctx.accounts.collection_authority.to_account_info(),
                ctx.accounts.collection_mint.to_account_info(),
                ctx.accounts.collection_metadata.to_account_info(),
                ctx.accounts.collection_master_edition.to_account_info(),
                ctx.accounts.token_metadata_program.to_account_info(),
            ],
            &[signer_seeds_collection_auth],
        )?;

        // 12) update affiliate stats PDA if affiliate exists
        if let Some(affiliate_pk) = affiliate_maybe {
            // affiliate_stats was declared with init_if_needed & seeds anchored to affiliate & campaign
            let stats = &mut ctx.accounts.affiliate_stats;
            stats.total_mints = stats.total_mints.checked_add(1).unwrap();
            stats.total_earned = stats.total_earned.checked_add(affiliate_cut).unwrap();
        }

        // 13) increment minted count
        campaign.minted = campaign.minted.checked_add(1).ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }
}

/// Account structs

#[derive(Accounts)]
#[instruction(price: u64, affiliate_fee_bps: u16)]
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

    /// collection mint (created previously by creator via Metaplex JS)
    pub collection_mint: UncheckedAccount<'info>,

    /// PDA used as program's collection authority (delegated by creator once off-chain)
    /// This account is a PDA derived as ["collection_auth", campaign.key()]
    /// It will be used to sign verify_collection CPI.
    /// Initialized by program via bump storage only — creator must call Metaplex to delegate authority.
    #[account(mut)]
    pub collection_authority: UncheckedAccount<'info>,

    /// PDA to be used as mint authority for minted NFTs
    /// Seeds: ["mint_auth", campaign.key()]
    /// Not stored as an account here, stored bump in campaign.
    #[account(mut)]
    pub mint_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ProcessMint<'info> {
    /// Buyer pays
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// The campaign PDA
    #[account(mut, has_one = collection_mint)]
    pub campaign: Account<'info, Campaign>,

    /// Creator (recipient)
    #[account(mut, address = campaign.creator)]
    pub creator: UncheckedAccount<'info>,

    /// Affiliate receiver (if affiliate provided)
    /// This account is used as a destination for transfer instruction (not signer)
    #[account(mut)]
    pub affiliate_receiver: UncheckedAccount<'info>,

    /// The program-derived mint PDA for this NFT (must be the PDA derived by program using campaign key + index)
    /// We don't `init` via attribute — the program creates it with system create_account & invoke_signed
    #[account(mut)]
    pub nft_mint: UncheckedAccount<'info>,

    /// Buyer ATA for the NFT (init_if_needed, payer = buyer)
    #[account(
        mut,
        associated_token::mint = nft_mint,
        associated_token::authority = buyer,
        init_if_needed,
        payer = buyer
    )]
    pub buyer_ata: Account<'info, TokenAccount>,

    /// Mint authority PDA (seeds ["mint_auth", campaign.key()])
    /// must equal the PDA computed by program
    /// we pass it so we can use it as `authority` in CPI and sign via seeds
    #[account(mut)]
    pub mint_authority: UncheckedAccount<'info>,

    /// Metadata account for the new NFT (must equal canonical metadata PDA)
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    /// Master edition account for the new NFT (must equal canonical edition PDA)
    #[account(mut)]
    pub master_edition: UncheckedAccount<'info>,

    /// Collection mint & its metadata/master edition (already created)
    pub collection_mint: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection_metadata: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection_master_edition: UncheckedAccount<'info>,

    /// Program that handles token metadata CPIs
    pub token_metadata_program: UncheckedAccount<'info>,

    /// Program PDAs:
    /// collection_authority PDA (seeds ["collection_auth", campaign.key()])
    /// used as signer when calling verify_sized_collection_item
    #[account(mut)]
    pub collection_authority: UncheckedAccount<'info>,

    /// Affiliate stats PDA (init_if_needed, payer = buyer)
    /// seeds = ["affiliate", campaign.key().as_ref(), affiliate_pubkey.as_ref()]
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
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Campaign layout
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

/// Affiliate stats account
#[account]
pub struct AffiliateStats {
    pub total_mints: u64,
    pub total_earned: u64,
}
impl AffiliateStats {
    pub const SIZE: usize = 8 + 8;
}

/// Validation helper
fn validate_accounts(ctx: &Context<ProcessMint>) -> Result<()> {
    // ensure metadata PDAs match canonical derivation
    let (metadata_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            mpl_token_metadata::id().as_ref(),
            ctx.accounts.nft_mint.key().as_ref(),
        ],
        &mpl_token_metadata::id(),
    );
    require!(metadata_pda == ctx.accounts.metadata.key(), ErrorCode::InvalidMetadata);

    let (edition_pda, _) = Pubkey::find_program_address(
        &[
            b"metadata",
            mpl_token_metadata::id().as_ref(),
            ctx.accounts.nft_mint.key().as_ref(),
            b"edition",
        ],
        &mpl_token_metadata::id(),
    );
    require!(edition_pda == ctx.accounts.master_edition.key(), ErrorCode::InvalidMasterEdition);

    Ok(())
}

/// Errors
#[error_code]
pub enum ErrorCode {
    #[msg("Invalid affiliate fee")]
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
}
