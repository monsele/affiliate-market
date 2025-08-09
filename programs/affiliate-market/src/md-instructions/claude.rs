use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use anchor_spl::associated_token::AssociatedToken;
use mpl_token_metadata::instruction as mpl_instruction;

declare_id!("Your_Program_ID_Here");

#[program]
pub mod affiliate_nft_dapp {
    use super::*;
    
    // Initialize the global program state
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let global_state = &mut ctx.accounts.global_state;
        global_state.authority = ctx.accounts.authority.key();
        global_state.campaign_count = 0;
        global_state.total_volume = 0;
        Ok(())
    }
    
    // Create a new campaign (NFT collection)
    pub fn create_campaign(
        ctx: Context<CreateCampaign>,
        name: String,
        description: String,
        image: String,
        base_uri: String,
        mint_price: u64,
        max_supply: u64,
        commission_rate: u16, // Basis points (e.g., 500 = 5%)
    ) -> Result<()> {
        let global_state = &mut ctx.accounts.global_state;
        let campaign = &mut ctx.accounts.campaign;
        
        campaign.creator = ctx.accounts.creator.key();
        campaign.campaign_id = global_state.campaign_count;
        campaign.name = name;
        campaign.description = description;
        campaign.image = image;
        campaign.base_uri = base_uri;
        campaign.mint_price = mint_price;
        campaign.max_supply = max_supply;
        campaign.current_supply = 0;
        campaign.commission_rate = commission_rate;
        campaign.is_active = true;
        campaign.created_at = Clock::get()?.unix_timestamp;
        
        global_state.campaign_count += 1;
        
        emit!(CampaignCreated {
            campaign_id: campaign.campaign_id,
            creator: campaign.creator,
            name: campaign.name.clone(),
            mint_price: campaign.mint_price,
            max_supply: campaign.max_supply,
            commission_rate: campaign.commission_rate,
        });
        
        Ok(())
    }
    
    // Optional: Create affiliate link (now optional since we use lazy init)
    pub fn create_affiliate_link(
        ctx: Context<CreateAffiliateLink>,
        campaign_id: u64,
    ) -> Result<()> {
        let affiliate_link = &mut ctx.accounts.affiliate_link;
        let campaign = &ctx.accounts.campaign;
        
        require!(campaign.is_active, ErrorCode::CampaignInactive);
        
        affiliate_link.affiliate = ctx.accounts.affiliate.key();
        affiliate_link.campaign_id = campaign_id;
        affiliate_link.total_mints = 0;
        affiliate_link.total_earnings = 0;
        affiliate_link.created_at = Clock::get()?.unix_timestamp;
        
        emit!(AffiliateLinkCreated {
            affiliate: affiliate_link.affiliate,
            campaign_id,
            link_id: affiliate_link.key(),
        });
        
        Ok(())
    }
    
    // Mint NFT with affiliate commission (with lazy initialization)
    pub fn mint_with_affiliate(
        ctx: Context<MintWithAffiliate>,
        campaign_id: u64,
        affiliate_pubkey: Pubkey, // Pass affiliate pubkey directly
        metadata_uri: String,
    ) -> Result<()> {
        let campaign = &mut ctx.accounts.campaign;
        
        require!(campaign.is_active, ErrorCode::CampaignInactive);
        require!(campaign.current_supply < campaign.max_supply, ErrorCode::MaxSupplyReached);
        
        // Initialize affiliate link if it doesn't exist (lazy initialization)
        let affiliate_link = &mut ctx.accounts.affiliate_link;
        let clock = Clock::get()?;
        
        // Check if this is the first time this affiliate link is used
        if affiliate_link.affiliate == Pubkey::default() {
            affiliate_link.affiliate = affiliate_pubkey;
            affiliate_link.campaign_id = campaign_id;
            affiliate_link.total_mints = 0;
            affiliate_link.total_earnings = 0;
            affiliate_link.created_at = clock.unix_timestamp;
            
            emit!(AffiliateLinkCreated {
                affiliate: affiliate_pubkey,
                campaign_id,
                link_id: affiliate_link.key(),
            });
        }
        
        // Verify the affiliate matches
        require!(affiliate_link.affiliate == affiliate_pubkey, ErrorCode::InvalidAffiliateLink);
        require!(affiliate_link.campaign_id == campaign_id, ErrorCode::InvalidAffiliateLink);
        
        // Calculate commission
        let commission_amount = (campaign.mint_price as u128 * campaign.commission_rate as u128) / 10000u128;
        let creator_amount = campaign.mint_price - commission_amount as u64;
        
        // Transfer payment to creator
        if creator_amount > 0 {
            let cpi_accounts = Transfer {
                from: ctx.accounts.buyer_token_account.to_account_info(),
                to: ctx.accounts.creator_token_account.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, creator_amount)?;
        }
        
        // Transfer commission to affiliate
        if commission_amount > 0 {
            let cpi_accounts = Transfer {
                from: ctx.accounts.buyer_token_account.to_account_info(),
                to: ctx.accounts.affiliate_token_account.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
            token::transfer(cpi_ctx, commission_amount as u64)?;
        }
        
        // Update campaign and affiliate stats
        campaign.current_supply += 1;
        affiliate_link.total_mints += 1;
        affiliate_link.total_earnings += commission_amount as u64;
        
        emit!(NFTMinted {
            campaign_id,
            buyer: ctx.accounts.buyer.key(),
            affiliate: affiliate_pubkey,
            mint_price: campaign.mint_price,
            commission: commission_amount as u64,
            token_id: campaign.current_supply,
        });
        
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + GlobalState::INIT_SPACE
    )]
    pub global_state: Account<'info, GlobalState>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(name: String, description: String, image: String, base_uri: String)]
pub struct CreateCampaign<'info> {
    #[account(mut)]
    pub global_state: Account<'info, GlobalState>,
    #[account(
        init,
        payer = creator,
        space = 8 + Campaign::INIT_SPACE,
        seeds = [b"campaign", global_state.campaign_count.to_le_bytes().as_ref()],
        bump
    )]
    pub campaign: Account<'info, Campaign>,
    #[account(mut)]
    pub creator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(campaign_id: u64)]
pub struct CreateAffiliateLink<'info> {
    pub campaign: Account<'info, Campaign>,
    #[account(
        init,
        payer = affiliate,
        space = 8 + AffiliateLink::INIT_SPACE,
        seeds = [b"affiliate", campaign.key().as_ref(), affiliate.key().as_ref()],
        bump
    )]
    pub affiliate_link: Account<'info, AffiliateLink>,
    #[account(mut)]
    pub affiliate: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(campaign_id: u64, affiliate_pubkey: Pubkey, metadata_uri: String)]
pub struct MintWithAffiliate<'info> {
    #[account(mut)]
    pub campaign: Account<'info, Campaign>,
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + AffiliateLink::INIT_SPACE,
        seeds = [b"affiliate", campaign.key().as_ref(), affiliate_pubkey.as_ref()],
        bump
    )]
    pub affiliate_link: Account<'info, AffiliateLink>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    /// CHECK: Creator account for receiving payment
    pub creator: AccountInfo<'info>,
    #[account(mut)]
    pub buyer_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub creator_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub affiliate_token_account: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct GlobalState {
    pub authority: Pubkey,
    pub campaign_count: u64,
    pub total_volume: u64,
}

#[account]
#[derive(InitSpace)]
pub struct Campaign {
    pub creator: Pubkey,
    pub campaign_id: u64,
    #[max_len(100)]
    pub name: String,
    #[max_len(500)]
    pub description: String,
    #[max_len(200)]
    pub image: String,
    #[max_len(200)]
    pub base_uri: String,
    pub mint_price: u64,
    pub max_supply: u64,
    pub current_supply: u64,
    pub commission_rate: u16, // Basis points
    pub is_active: bool,
    pub created_at: i64,
}

#[account]
#[derive(InitSpace)]
pub struct AffiliateLink {
    pub affiliate: Pubkey,
    pub campaign_id: u64,
    pub total_mints: u64,
    pub total_earnings: u64,
    pub created_at: i64,
}

#[event]
pub struct CampaignCreated {
    pub campaign_id: u64,
    pub creator: Pubkey,
    pub name: String,
    pub mint_price: u64,
    pub max_supply: u64,
    pub commission_rate: u16,
}

#[event]
pub struct AffiliateLinkCreated {
    pub affiliate: Pubkey,
    pub campaign_id: u64,
    pub link_id: Pubkey,
}

#[event]
pub struct NFTMinted {
    pub campaign_id: u64,
    pub buyer: Pubkey,
    pub affiliate: Pubkey,
    pub mint_price: u64,
    pub commission: u64,
    pub token_id: u64,
}

#[error_code]
pub enum ErrorCode {
    #[msg("Campaign is not active")]
    CampaignInactive,
    #[msg("Maximum supply reached")]
    MaxSupplyReached,
    #[msg("Invalid affiliate link")]
    InvalidAffiliateLink,
}