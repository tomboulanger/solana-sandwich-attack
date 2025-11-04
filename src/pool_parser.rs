use crate::types::{
    DexType, PoolInfo, RaydiumAmmInfo, OrcaWhirlpoolInfo, 
    MeteoraDLMMInfo, LifinityPoolInfo, PhoenixMarketInfo, SerumMarketInfo,
    WSOL_MINT, USDC_MINT, USDT_MINT
};
use solana_sdk::pubkey::Pubkey;
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use spl_token::state::Account as TokenAccount;
use solana_sdk::program_pack::Pack;
use borsh::BorshDeserialize;
use anyhow::{Result, anyhow};
use std::str::FromStr;
use std::sync::Arc;

// ============================================================================
// POOL PARSER - GESTION DE TOUS LES TYPES DE POOLS
// ============================================================================

pub struct PoolParser {
    pub async_rpc: Arc<AsyncRpcClient>,
    pub sol_price_usd: f64,
}

impl PoolParser {
    pub fn new(async_rpc: Arc<AsyncRpcClient>) -> Self {
        Self {
            async_rpc,
            sol_price_usd: 150.0, // Prix par défaut, sera mis à jour
        }
    }

    pub fn set_sol_price(&mut self, price: f64) {
        self.sol_price_usd = price;
    }

    /// Parse un pool en fonction du type de DEX
    pub async fn parse_pool(&self, pool_id: &Pubkey, dex_type: DexType, program_id: Pubkey) -> Result<PoolInfo> {
        let account = self.async_rpc.get_account(pool_id).await?;
        
        match dex_type {
            DexType::RaydiumV4 => self.parse_raydium_v4(&account.data, *pool_id, program_id).await,
            DexType::OrcaWhirlpool => self.parse_orca_whirlpool(&account.data, *pool_id, program_id).await,
            DexType::MeteoraDLMM => self.parse_meteora_dlmm(&account.data, *pool_id, program_id).await,
            DexType::Lifinity => self.parse_lifinity(&account.data, *pool_id, program_id).await,
            DexType::Phoenix => self.parse_phoenix(&account.data, *pool_id, program_id).await,
            DexType::Serum => self.parse_serum(&account.data, *pool_id, program_id).await,
            DexType::Jupiter => Err(anyhow!("Jupiter est un agrégateur, pas un pool direct")),
            DexType::Unsupported => Err(anyhow!("Type de DEX non supporté")),
            DexType::Unknown => Err(anyhow!("Type de DEX inconnu")),
        }
    }

    // ============================================================================
    // RAYDIUM V4 PARSER
    // ============================================================================
    
    async fn parse_raydium_v4(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let amm_info = RaydiumAmmInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Raydium V4: {}", e))?;

        // Récupérer les réserves
        let reserve_a = self.get_token_balance(&amm_info.base_vault).await?;
        let reserve_b = self.get_token_balance(&amm_info.quote_vault).await?;

        // Calculer les frais
        let fee_bps = if amm_info.swap_fee_denominator > 0 {
            (amm_info.swap_fee_numerator as f64 / amm_info.swap_fee_denominator as f64 * 10000.0) as u16
        } else {
            25 // Frais par défaut
        };

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &amm_info.base_mint,
                &amm_info.quote_mint,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::RaydiumV4,
            program_id,
            pool_id,
            token_a_mint: amm_info.base_mint,
            token_b_mint: amm_info.quote_mint,
            token_a_vault: amm_info.base_vault,
            token_b_vault: amm_info.quote_vault,
            reserve_a,
            reserve_b,
            fee_bps,
            tick_spacing: None,
            tick_current: None,
            bin_step: None,
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // ORCA WHIRLPOOL PARSER
    // ============================================================================
    
    async fn parse_orca_whirlpool(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let whirlpool = OrcaWhirlpoolInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Orca Whirlpool: {}", e))?;

        // Utiliser les montants des vaults directement depuis la structure
        let reserve_a = whirlpool.token_vault_a_amount;
        let reserve_b = whirlpool.token_vault_b_amount;

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &whirlpool.token_mint_a,
                &whirlpool.token_mint_b,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::OrcaWhirlpool,
            program_id,
            pool_id,
            token_a_mint: whirlpool.token_mint_a,
            token_b_mint: whirlpool.token_mint_b,
            token_a_vault: whirlpool.token_vault_a,
            token_b_vault: whirlpool.token_vault_b,
            reserve_a,
            reserve_b,
            fee_bps: whirlpool.fee_rate,
            tick_spacing: Some(whirlpool.tick_spacing as i32),
            tick_current: Some(whirlpool.tick_current_index),
            bin_step: None,
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // METEORA DLMM PARSER
    // ============================================================================
    
    async fn parse_meteora_dlmm(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let dlmm = MeteoraDLMMInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Meteora DLMM: {}", e))?;

        // Récupérer les réserves
        let reserve_a = self.get_token_balance(&dlmm.reserve_x).await?;
        let reserve_b = self.get_token_balance(&dlmm.reserve_y).await?;

        // Calculer les frais totaux
        let fee_bps = dlmm.protocol_fee_bps + dlmm.base_fee_bps;

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &dlmm.mint_x,
                &dlmm.mint_y,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::MeteoraDLMM,
            program_id,
            pool_id,
            token_a_mint: dlmm.mint_x,
            token_b_mint: dlmm.mint_y,
            token_a_vault: dlmm.reserve_x,
            token_b_vault: dlmm.reserve_y,
            reserve_a,
            reserve_b,
            fee_bps,
            tick_spacing: None,
            tick_current: Some(dlmm.active_id),
            bin_step: Some(dlmm.bin_step),
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // LIFINITY PARSER
    // ============================================================================
    
    async fn parse_lifinity(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let lifinity = LifinityPoolInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Lifinity: {}", e))?;

        // Récupérer les réserves
        let reserve_a = self.get_token_balance(&lifinity.token_a_vault).await?;
        let reserve_b = self.get_token_balance(&lifinity.token_b_vault).await?;

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &lifinity.token_a_mint,
                &lifinity.token_b_mint,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::Lifinity,
            program_id,
            pool_id,
            token_a_mint: lifinity.token_a_mint,
            token_b_mint: lifinity.token_b_mint,
            token_a_vault: lifinity.token_a_vault,
            token_b_vault: lifinity.token_b_vault,
            reserve_a,
            reserve_b,
            fee_bps: lifinity.fee_rate,
            tick_spacing: None,
            tick_current: None,
            bin_step: None,
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // PHOENIX PARSER
    // ============================================================================
    
    async fn parse_phoenix(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let phoenix = PhoenixMarketInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Phoenix: {}", e))?;

        // Récupérer les réserves
        let reserve_a = self.get_token_balance(&phoenix.base_vault).await?;
        let reserve_b = self.get_token_balance(&phoenix.quote_vault).await?;

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &phoenix.base_mint,
                &phoenix.quote_mint,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::Phoenix,
            program_id,
            pool_id,
            token_a_mint: phoenix.base_mint,
            token_b_mint: phoenix.quote_mint,
            token_a_vault: phoenix.base_vault,
            token_b_vault: phoenix.quote_vault,
            reserve_a,
            reserve_b,
            fee_bps: phoenix.taker_fee_bps,
            tick_spacing: None,
            tick_current: None,
            bin_step: None,
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // SERUM PARSER
    // ============================================================================
    
    async fn parse_serum(&self, data: &[u8], pool_id: Pubkey, program_id: Pubkey) -> Result<PoolInfo> {
        let serum = SerumMarketInfo::try_from_slice(data)
            .map_err(|e| anyhow!("Erreur parsing Serum: {}", e))?;

        // Récupérer les réserves
        let reserve_a = self.get_token_balance(&serum.base_vault).await?;
        let reserve_b = self.get_token_balance(&serum.quote_vault).await?;

        // Calculer la liquidité et le market cap
        let (liquidity_usd, token_a_liquidity, token_b_liquidity, market_cap_usd, token_price_usd, total_supply) = 
            self.calculate_pool_metrics(
                &serum.base_mint,
                &serum.quote_mint,
                reserve_a,
                reserve_b,
            ).await?;

        Ok(PoolInfo {
            dex_type: DexType::Serum,
            program_id,
            pool_id,
            token_a_mint: serum.base_mint,
            token_b_mint: serum.quote_mint,
            token_a_vault: serum.base_vault,
            token_b_vault: serum.quote_vault,
            reserve_a,
            reserve_b,
            fee_bps: 22, // Serum a généralement 0.22% de frais
            tick_spacing: None,
            tick_current: None,
            bin_step: None,
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        })
    }

    // ============================================================================
    // FONCTIONS UTILITAIRES
    // ============================================================================

    /// Récupère la balance d'un token account
    async fn get_token_balance(&self, token_account: &Pubkey) -> Result<u64> {
        let account_data = self.async_rpc.get_account(token_account).await?;
        let token_account = TokenAccount::unpack(&account_data.data)?;
        Ok(token_account.amount)
    }

    /// Récupère le total supply d'un token
    async fn get_token_supply(&self, mint: &Pubkey) -> Result<u64> {
        let supply = self.async_rpc.get_token_supply(mint).await?;
        Ok(supply.amount.parse::<u64>()?)
    }

    /// Calcule toutes les métriques du pool (liquidité, mcap, prix)
    async fn calculate_pool_metrics(
        &self,
        token_a_mint: &Pubkey,
        token_b_mint: &Pubkey,
        reserve_a: u64,
        reserve_b: u64,
    ) -> Result<(f64, f64, f64, Option<f64>, Option<f64>, Option<u64>)> {
        
        let wsol_mint = Pubkey::from_str(WSOL_MINT)?;
        let usdc_mint = Pubkey::from_str(USDC_MINT)?;

        // Déterminer quel token est SOL/USDC et lequel est le token custom
        let (is_a_stable, is_b_stable) = (
            *token_a_mint == wsol_mint || *token_a_mint == usdc_mint,
            *token_b_mint == wsol_mint || *token_b_mint == usdc_mint,
        );

        // Calculer la liquidité en USD
        let mut liquidity_usd;
        let token_a_liquidity = reserve_a as f64 / 1e9; // Assuming 9 decimals
        let token_b_liquidity = reserve_b as f64 / 1e9;

        // Calculer le prix et le market cap
        let mut token_price_usd = None;
        let mut market_cap_usd = None;
        let mut total_supply = None;

        if is_a_stable && !is_b_stable {
            // Token A est stable (SOL/USDC), Token B est le custom token
            let stable_value = if *token_a_mint == wsol_mint {
                token_a_liquidity * self.sol_price_usd
            } else {
                token_a_liquidity // USDC vaut 1 USD
            };

            liquidity_usd = stable_value * 2.0; // TVL totale = 2x la valeur stable

            // Calculer le prix du token custom
            if reserve_b > 0 {
                let price = (reserve_a as f64 / reserve_b as f64) * 
                    if *token_a_mint == wsol_mint { self.sol_price_usd } else { 1.0 };
                token_price_usd = Some(price);

                // Récupérer le supply total et calculer le mcap
                if let Ok(supply) = self.get_token_supply(token_b_mint).await {
                    total_supply = Some(supply);
                    market_cap_usd = Some((supply as f64 / 1e9) * price);
                }
            }

        } else if !is_a_stable && is_b_stable {
            // Token B est stable (SOL/USDC), Token A est le custom token
            let stable_value = if *token_b_mint == wsol_mint {
                token_b_liquidity * self.sol_price_usd
            } else {
                token_b_liquidity // USDC vaut 1 USD
            };

            liquidity_usd = stable_value * 2.0; // TVL totale = 2x la valeur stable

            // Calculer le prix du token custom
            if reserve_a > 0 {
                let price = (reserve_b as f64 / reserve_a as f64) * 
                    if *token_b_mint == wsol_mint { self.sol_price_usd } else { 1.0 };
                token_price_usd = Some(price);

                // Récupérer le supply total et calculer le mcap
                if let Ok(supply) = self.get_token_supply(token_a_mint).await {
                    total_supply = Some(supply);
                    market_cap_usd = Some((supply as f64 / 1e9) * price);
                }
            }

        } else if is_a_stable && is_b_stable {
            // Les deux sont stables (SOL-USDC pool par exemple)
            let value_a = if *token_a_mint == wsol_mint {
                token_a_liquidity * self.sol_price_usd
            } else {
                token_a_liquidity
            };
            
            let value_b = if *token_b_mint == wsol_mint {
                token_b_liquidity * self.sol_price_usd
            } else {
                token_b_liquidity
            };

            liquidity_usd = value_a + value_b;
        } else {
            // Pool entre deux tokens customs - estimer la liquidité
            // Utiliser une heuristique basique
            liquidity_usd = (reserve_a as f64 + reserve_b as f64) / 1e9 * 0.1; // Estimation très approximative
        }

        Ok((
            liquidity_usd,
            token_a_liquidity,
            token_b_liquidity,
            market_cap_usd,
            token_price_usd,
            total_supply,
        ))
    }

    /// Vérifie si un pool est valide pour le sandwich attack
    pub fn is_pool_valid_for_sandwich(&self, pool: &PoolInfo, min_liquidity: f64, max_liquidity: f64) -> bool {
        // Vérifier la liquidité
        if pool.liquidity_usd < min_liquidity || pool.liquidity_usd > max_liquidity {
            return false;
        }

        // Vérifier que les réserves sont suffisantes
        if pool.reserve_a == 0 || pool.reserve_b == 0 {
            return false;
        }

        // Vérifier le market cap si disponible
        if let Some(mcap) = pool.market_cap_usd {
            // Éviter les tokens avec un mcap trop faible (probable scam) ou trop élevé (pas rentable)
            if mcap < 10_000.0 || mcap > 10_000_000.0 {
                return false;
            }
        }

        true
    }

    /// Calcule l'impact sur le prix d'un swap
    pub fn calculate_price_impact(&self, pool: &PoolInfo, amount_in: u64, is_a_to_b: bool) -> f64 {
        let (reserve_in, reserve_out) = if is_a_to_b {
            (pool.reserve_a, pool.reserve_b)
        } else {
            (pool.reserve_b, pool.reserve_a)
        };

        if reserve_in == 0 || reserve_out == 0 {
            return 0.0;
        }

        // Formule AMM : x * y = k
        let k = (reserve_in as f64) * (reserve_out as f64);
        let new_reserve_in = reserve_in as f64 + amount_in as f64;
        let new_reserve_out = k / new_reserve_in;
        
        let price_before = reserve_out as f64 / reserve_in as f64;
        let price_after = new_reserve_out / new_reserve_in;
        
        let impact = ((price_after - price_before) / price_before).abs() * 100.0;
        impact
    }
}

