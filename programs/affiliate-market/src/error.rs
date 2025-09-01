use anchor_lang::error_code;


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