use solana_sdk::pubkey::Pubkey;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Serialize, Deserialize};
use std::time::{Duration, Instant};

// ============================================================================
// PROGRAM IDs
// ============================================================================
pub const JUPITER_V6: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
pub const TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCwuBvf9Sg8ePdLA";
pub const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
pub const USDC_MINT: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";
pub const USDT_MINT: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";


// Jito tip accounts
pub const JITO_TIP_ACCOUNTS: &[&str] = &[
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
];

// ============================================================================
// STRUCTURES
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum DexType {
    RaydiumV4,
    OrcaWhirlpool,
    MeteoraDLMM,
    Lifinity,
    Phoenix,
    Serum,
    Jupiter,
    Unsupported,  // DEX connu mais non supporté
    Unknown,      // DEX complètement inconnu
}

#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub dex_type: DexType,
    pub program_id: Pubkey,
    pub pool_id: Pubkey,
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub token_a_vault: Pubkey,
    pub token_b_vault: Pubkey,
    pub reserve_a: u64,
    pub reserve_b: u64,
    pub fee_bps: u16,
    pub tick_spacing: Option<i32>,
    pub tick_current: Option<i32>,
    pub bin_step: Option<u16>,
    
    // Nouvelles informations de liquidité et market cap
    pub liquidity_usd: f64,
    pub token_a_liquidity: f64,
    pub token_b_liquidity: f64,
    pub market_cap_usd: Option<f64>,
    pub token_price_usd: Option<f64>,
    pub total_supply: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ParsedSwap {
    pub signature: String,
    pub user: Pubkey,
    pub pool: PoolInfo,
    pub amount_in: u64,
    pub amount_out_min: u64,
    pub token_in: Pubkey,
    pub token_out: Pubkey,
    pub timestamp: Instant,
    pub a_to_b: bool,
}

#[derive(Debug)]
pub struct ProfitAnalysis {
    pub is_profitable: bool,
    pub profit_lamports: u64,
    pub profit_percent: f64,
    pub front_run_amount: u64,
    pub back_run_amount_min: u64,
    pub price_impact_bps: u64,
    pub gas_cost_lamports: u64,
}

#[derive(Debug)]
pub struct SwapSimulation {
    pub tokens_out: u64,
    pub tokens_out_min: u64,
    pub price_impact_bps: u64,
}

#[derive(Debug)]
pub struct ParsedSwapInstruction {
    pub pool_id: Pubkey,
    pub user: Pubkey,
    pub amount_in: u64,
    pub amount_out_min: u64,
    pub token_in: Pubkey,
    pub token_out: Pubkey,
    pub a_to_b: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BundleStatus {
    pub bundle_id: String,
    pub status: String,
    pub landed_slot: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionLog {
    pub timestamp: String,
    pub signature: String,
    pub pool_id: String,
    pub dex_type: String,
    
    // Détails de la transaction
    pub user: String,
    pub token_in: String,
    pub token_out: String,
    pub amount_in: u64,
    pub amount_out_min: u64,
    pub a_to_b: bool,
    
    // Informations sur le pool
    pub pool_reserve_a: u64,
    pub pool_reserve_b: u64,
    pub pool_fee_bps: u64,
    
    // Estimation du prix
    pub price_before: f64,
    pub price_after: f64,
    pub price_impact_pct: f64,
    
    // Capitalisation estimée
    pub estimated_mcap_before: f64,
    pub estimated_mcap_after: f64,
    
    // Analyse de rentabilité
    pub our_position_size: u64,
    pub estimated_profit_pct: f64,
    pub estimated_profit_lamports: u64,
    pub gas_cost_lamports: u64,
    
    // Liquidity
    pub liquidity_usd: f64,
    
    // Status
    pub bundle_id: Option<String>,
    pub success: bool,
    pub failure_reason: Option<String>,
}

// ============================================================================
// RAYDIUM V4 STRUCTURES
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct RaydiumAmmInfo {
    pub status: u64,
    pub nonce: u64,
    pub max_order: u64,
    pub depth: u64,
    pub base_decimal: u64,
    pub quote_decimal: u64,
    pub state: u64,
    pub reset_flag: u64,
    pub min_size: u64,
    pub vol_max_cut_ratio: u64,
    pub amount_wave_ratio: u64,
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub min_price_multiplier: u64,
    pub max_price_multiplier: u64,
    pub system_decimal_value: u64,
    pub min_separate_numerator: u64,
    pub min_separate_denominator: u64,
    pub trade_fee_numerator: u64,
    pub trade_fee_denominator: u64,
    pub pnl_numerator: u64,
    pub pnl_denominator: u64,
    pub swap_fee_numerator: u64,
    pub swap_fee_denominator: u64,
    pub base_need_take_pnl: u64,
    pub quote_need_take_pnl: u64,
    pub quote_total_pnl: u64,
    pub base_total_pnl: u64,
    pub pool_open_time: u64,
    pub punish_pc_amount: u64,
    pub punish_coin_amount: u64,
    pub orderbook_to_init_time: u64,
    pub swap_base_in_amount: u128,
    pub swap_quote_out_amount: u128,
    pub swap_base2_quote_fee: u64,
    pub swap_quote_in_amount: u128,
    pub swap_base_out_amount: u128,
    pub swap_quote2_base_fee: u64,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub open_orders: Pubkey,
    pub market_id: Pubkey,
    pub market_program_id: Pubkey,
    pub target_orders: Pubkey,
    pub withdraw_queue: Pubkey,
    pub token_temp_lp: Pubkey,
    pub amm_owner: Pubkey,
    pub lp_amount: u64,
}

// ============================================================================
// ORCA WHIRLPOOL STRUCTURES
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct OrcaWhirlpoolInfo {
    pub start_tick_index: i32,
    pub tick_spacing: u16,
    pub fee_rate: u16,
    pub protocol_fee_rate: u16,
    pub liquidity: u128,
    pub sqrt_price: u128,
    pub tick_current_index: i32,
    pub protocol_fee_owed_a: u64,
    pub protocol_fee_owed_b: u64,
    pub token_mint_a: Pubkey,
    pub token_vault_a: Pubkey,
    pub fee_growth_global_a: u128,
    pub token_mint_b: Pubkey,
    pub token_vault_b: Pubkey,
    pub fee_growth_global_b: u128,
    pub reward_last_updated_timestamp: u64,
    pub reward_infos: [OrcaRewardInfo; 3],
    pub token_vault_a_amount: u64,
    pub token_vault_b_amount: u64,
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct OrcaRewardInfo {
    pub mint: Pubkey,
    pub vault: Pubkey,
    pub authority: Pubkey,
    pub emissions_per_second_x64: u128,
    pub growth_global_x64: u128,
}

// ============================================================================
// METEORA DLMM STRUCTURES
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct MeteoraDLMMInfo {
    pub bin_step: u16,
    pub active_id: i32,
    pub protocol_fee_bps: u16,
    pub base_fee_bps: u16,
    pub reserve_x: Pubkey,
    pub reserve_y: Pubkey,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    pub oracle_id: i32,
    pub liquidity: u128,
    pub bin_liquidity: u128,
}

// ============================================================================
// LIFINITY STRUCTURES  
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct LifinityPoolInfo {
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub token_a_vault: Pubkey,
    pub token_b_vault: Pubkey,
    pub fee_rate: u16,
    pub oracle: Pubkey,
}

// ============================================================================
// PHOENIX STRUCTURES
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct PhoenixMarketInfo {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub tick_size: u64,
    pub taker_fee_bps: u16,
}

// ============================================================================
// SERUM STRUCTURES
// ============================================================================
#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct SerumMarketInfo {
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub base_lot_size: u64,
    pub quote_lot_size: u64,
    pub vault_signer_nonce: u64,
}

#[derive(Debug, Clone)]
pub struct SandwichAnalysisResult {
    pub signature: String,
    pub invested_amount: f64,
    pub tokens_received: f64,
    pub mcap_before: f64,
    pub mcap_after: f64,
    pub mcap_impact: f64,
    pub execution_time: Duration,
    pub is_sandwich_opportunity: bool,
    pub estimated_profit: f64,
}
