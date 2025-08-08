use anchor_lang::prelude::*;

declare_id!("6jxp4eoRZ8C7qVeXKyHk68YEmCoBVHR1AQxJ9Le4Aey1");

#[program]
pub mod affiliate_market {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
