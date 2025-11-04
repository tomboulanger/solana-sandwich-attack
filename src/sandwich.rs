use crate::config::BotConfig;
use crate::monitoring::MonitoringEngine;
use crate::types::{
    DexType, PoolInfo, ParsedSwap, ProfitAnalysis, SwapSimulation, TransactionLog,
};
use anyhow::{Result, anyhow};
use solana_sdk::{
    pubkey::Pubkey,
    transaction::Transaction,
    message::Message,
    signature::{Keypair, Signer},
    compute_budget::ComputeBudgetInstruction,
};
use solana_client::{
    rpc_client::RpcClient,
    nonblocking::rpc_client::RpcClient as AsyncRpcClient,
};
use std::sync::Arc;
use ahash::AHashMap;
use std::fs::OpenOptions;
use std::io::Write;
use chrono;
use tokio::time::{Duration, Instant};

// ============================================================================
// SANDWICH ENGINE COMPLET
// ============================================================================

pub struct SandwichEngine {
    pub config: Arc<BotConfig>,
    pub monitoring_engine: Arc<MonitoringEngine>,
    pub rpc: Arc<RpcClient>,
    pub async_rpc: Arc<AsyncRpcClient>,
    pub user_token_accounts: AHashMap<Pubkey, Pubkey>,
    pub wallet_keypair: Keypair,
}

impl SandwichEngine {
    pub fn new(
        config: Arc<BotConfig>,
        monitoring_engine: Arc<MonitoringEngine>,
        rpc: Arc<RpcClient>,
        async_rpc: Arc<AsyncRpcClient>,
        user_token_accounts: AHashMap<Pubkey, Pubkey>,
        wallet_keypair: Keypair,
    ) -> Self {
        Self {
            config,
            monitoring_engine,
            rpc,
            async_rpc,
            user_token_accounts,
            wallet_keypair,
        }
    }

    /// Détecte une opportunité de sandwich et l'exécute
    pub async fn detect_and_execute_sandwich(&self, target_tx_signature: &str) -> Result<String> {
        let start_time = Instant::now();

        // 1. Analyser la transaction cible rapidement
        let (tokens_received, _mcap_before, mcap_impact_pct) = self.monitoring_engine
            .calculate_tokens_received_and_mcap_impact(target_tx_signature, 0.0)
            .await?;

        log::info!("🎯 Analyse rapide - Impact: {:.2}%, Tokens: {:.0}", mcap_impact_pct, tokens_received);

        // 2. Vérifier si c'est une opportunité rentable
        let min_impact = 0.5; // 0.5% minimum
        if mcap_impact_pct < min_impact {
            return Err(anyhow!("Impact trop faible: {:.2}% < {:.2}%", mcap_impact_pct, min_impact));
        }

        // 3. Calculer les quantités pour le sandwich
        let front_run_amount = tokens_received * 0.1; // 10% de la transaction cible
        let back_run_amount = tokens_received * 0.1; // 10% de la transaction cible

        // 4. Créer les transactions avec priorité maximale
        let front_run_tx = self.create_front_run_transaction(target_tx_signature, front_run_amount).await?;
        let back_run_tx = self.create_back_run_transaction(target_tx_signature, back_run_amount).await?;

        // 5. Créer le bundle atomique
        let bundle = self.create_atomic_bundle(front_run_tx, back_run_tx).await?;

        // 6. Soumettre le bundle rapidement
        let signature = self.submit_bundle_with_retry(bundle).await?;

        let total_time = start_time.elapsed();
        log::info!("⚡ Sandwich exécuté en {}ms", total_time.as_millis());

        Ok(signature)
    }

    /// Crée une transaction front-run (achat avant la cible)
    async fn create_front_run_transaction(
        &self,
        target_tx_signature: &str,
        amount: f64,
    ) -> Result<Transaction> {
        log::info!("🏗️ Construction front-run - Target: {}, Amount: {:.0}", target_tx_signature, amount);
        
        // TODO: Implémenter la construction de transaction front-run
        // Pour l'instant, créer une transaction vide avec priorité maximale
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_price(100_000),
            ComputeBudgetInstruction::set_compute_unit_limit(200_000),
        ];
        
        let message = Message::new(&instructions, Some(&self.wallet_keypair.pubkey()));
        Ok(Transaction::new_unsigned(message))
    }

    /// Crée une transaction back-run (vente après la cible)
    async fn create_back_run_transaction(
        &self,
        target_tx_signature: &str,
        amount: f64,
    ) -> Result<Transaction> {
        log::info!("🏗️ Construction back-run - Target: {}, Amount: {:.0}", target_tx_signature, amount);
        
        // TODO: Implémenter la construction de transaction back-run
        // Pour l'instant, créer une transaction vide avec priorité maximale
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_price(100_000),
            ComputeBudgetInstruction::set_compute_unit_limit(200_000),
        ];
        
        let message = Message::new(&instructions, Some(&self.wallet_keypair.pubkey()));
        Ok(Transaction::new_unsigned(message))
    }

    /// Crée un bundle atomique avec les transactions front-run et back-run
    async fn create_atomic_bundle(
        &self,
        front_run_tx: Transaction,
        back_run_tx: Transaction,
    ) -> Result<Vec<Transaction>> {
        // 1. Utiliser le même recent_blockhash pour toutes les transactions
        let recent_blockhash = self.rpc.get_latest_blockhash()?;
        
        // 2. Créer un bundle avec les 2 transactions
        let mut bundle = vec![front_run_tx, back_run_tx];
        
        // 3. Signer toutes les transactions avec le même blockhash
        for tx in &mut bundle {
            tx.sign(&[&self.wallet_keypair], recent_blockhash);
        }
        
        Ok(bundle)
    }

    /// Soumet le bundle avec retry automatique
    async fn submit_bundle_with_retry(&self, bundle: Vec<Transaction>) -> Result<String> {
        let max_retries = 3;
        let mut retry_count = 0;
        
        while retry_count < max_retries {
            match self.try_submit_bundle(&bundle).await {
                Ok(signature) => return Ok(signature),
                Err(e) => {
                    retry_count += 1;
                    log::warn!("Tentative {} échouée: {}", retry_count, e);
                    
                    if retry_count < max_retries {
                        // Attendre un peu avant de retry
                        tokio::time::sleep(Duration::from_millis(50)).await;
                    }
                }
            }
        }
        
        Err(anyhow!("Échec après {} tentatives", max_retries))
    }

    /// Essaie de soumettre le bundle
    async fn try_submit_bundle(&self, bundle: &[Transaction]) -> Result<String> {
        // Soumettre la première transaction (front-run)
        if let Some(front_run_tx) = bundle.first() {
            let signature = self.rpc.send_and_confirm_transaction(front_run_tx)?;
            log::info!("🚀 Front-run soumis: {}", signature);
            
            // Soumettre la deuxième transaction (back-run) immédiatement
            if let Some(back_run_tx) = bundle.get(1) {
                let back_signature = self.rpc.send_and_confirm_transaction(back_run_tx)?;
                log::info!("🚀 Back-run soumis: {}", back_signature);
                return Ok(back_signature.to_string());
            }
            
            return Ok(signature.to_string());
        }
        
        Err(anyhow!("Bundle vide"))
    }

    /// Exécute un sandwich attack complet
    pub async fn execute_sandwich_attack(&self, target_tx_signature: &str) -> Result<String> {
        let start_time = Instant::now();

        // 1. Analyser l'opportunité rapidement (< 50ms)
        let (tokens_received, _mcap_before, mcap_impact_pct) = self.monitoring_engine
            .calculate_tokens_received_and_mcap_impact(target_tx_signature, 0.0)
            .await?;
        
        let min_impact = 0.5; // 0.5% minimum
        if mcap_impact_pct < min_impact {
            return Err(anyhow!("Impact trop faible: {:.2}%", mcap_impact_pct));
        }
        
        // 2. Créer les transactions avec priorité maximale
        let front_run_amount = tokens_received * 0.1; // 10% de la transaction cible
        let back_run_amount = tokens_received * 0.1; // 10% de la transaction cible
        
        let front_run_tx = self.create_front_run_transaction(target_tx_signature, front_run_amount).await?;
        let back_run_tx = self.create_back_run_transaction(target_tx_signature, back_run_amount).await?;
        
        // 3. Créer le bundle atomique
        let bundle = self.create_atomic_bundle(front_run_tx, back_run_tx).await?;
        
        // 4. Soumettre le bundle rapidement
        let signature = self.submit_bundle_with_retry(bundle).await?;
        
        let total_time = start_time.elapsed();
        log::info!("🎯 Sandwich attack exécuté en {}ms: {}", total_time.as_millis(), signature);
        
        Ok(signature)
    }

    // ============================================================================
    // FONCTIONS EXISTANTES (gardées pour compatibilité)
    // ============================================================================

    pub async fn analyze_profitability(&self, swap: &ParsedSwap) -> Result<ProfitAnalysis> {
        let pool = &swap.pool;

        // Pour les tokens small cap, analyser la capitalisation
        if let Some(mcap) = self.estimate_token_mcap(pool).await? {
            if mcap < self.config.min_mcap_usd || mcap > self.config.max_mcap_usd {
                log::debug!("Token mcap {} USD hors range [{}, {}]", 
                    mcap, self.config.min_mcap_usd, self.config.max_mcap_usd);
                return Ok(ProfitAnalysis {
                    is_profitable: false,
                    profit_lamports: 0,
                    profit_percent: 0.0,
                    front_run_amount: 0,
                    back_run_amount_min: 0,
                    price_impact_bps: 0,
                    gas_cost_lamports: 0,
                });
            }
            log::info!("🎯 Small Cap Token détecté - MCap: ${:.0}", mcap);
        }

        // Simuler le sandwich attack
        let simulation = self.simulate_sandwich_attack(swap).await?;

        Ok(ProfitAnalysis {
            is_profitable: simulation.tokens_out > 0,
            profit_lamports: simulation.tokens_out,
            profit_percent: 0.0, // TODO: Calculer le pourcentage
            front_run_amount: simulation.tokens_out_min,
            back_run_amount_min: simulation.tokens_out_min,
            price_impact_bps: simulation.price_impact_bps,
            gas_cost_lamports: 0, // TODO: Calculer le coût du gas
        })
    }

    async fn estimate_token_mcap(&self, _pool: &PoolInfo) -> Result<Option<f64>> {
        // TODO: Implémenter l'estimation de MCap
        Ok(None)
    }

    async fn simulate_sandwich_attack(&self, _swap: &ParsedSwap) -> Result<SwapSimulation> {
        // TODO: Implémenter la simulation
        Ok(SwapSimulation {
            tokens_out: 0,
            tokens_out_min: 0,
            price_impact_bps: 0,
        })
    }

    pub async fn calculate_profit_for_swap(&self, swap: &ParsedSwap) -> Result<SwapSimulation> {
        match swap.pool.dex_type {
            DexType::RaydiumV4 => {
                // TODO: Implémenter le calcul pour Raydium V4
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::OrcaWhirlpool => {
                // TODO: Implémenter le calcul pour Orca Whirlpool
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::MeteoraDLMM => {
                // TODO: Implémenter le calcul pour Meteora DLMM
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::Lifinity => {
                // TODO: Implémenter le calcul pour Lifinity
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::Phoenix => {
                // TODO: Implémenter le calcul pour Phoenix
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::Serum => {
                // TODO: Implémenter le calcul pour Serum
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::Jupiter => {
                // TODO: Implémenter le calcul pour Jupiter
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
            DexType::Unsupported => {
                // DEX non supporté - ne peut pas calculer
                Err(anyhow!("DEX non supporté pour le calcul de profit"))
            }
            DexType::Unknown => {
                // TODO: Implémenter le calcul pour DEX inconnu
                Ok(SwapSimulation {
                    tokens_out: 0,
                    tokens_out_min: 0,
                    price_impact_bps: 0,
                })
            }
        }
    }

    pub async fn build_transaction_log(&self, swap: &ParsedSwap, profit: &SwapSimulation) -> Result<TransactionLog> {
        let pool = &swap.pool;
        let pool_fee_bps = pool.fee_bps as u64;
        
        Ok(TransactionLog {
            timestamp: chrono::Utc::now().to_string(),
            signature: "".to_string(), // Sera rempli après soumission
            pool_id: pool.pool_id.to_string(),
            dex_type: format!("{:?}", pool.dex_type),
            user: swap.user.to_string(),
            token_in: swap.token_in.to_string(),
            token_out: swap.token_out.to_string(),
            amount_in: swap.amount_in,
            amount_out_min: swap.amount_out_min,
            a_to_b: swap.a_to_b,
            pool_reserve_a: pool.reserve_a,
            pool_reserve_b: pool.reserve_b,
            pool_fee_bps,
            price_before: 0.0, // TODO: Calculer
            price_after: 0.0, // TODO: Calculer
            price_impact_pct: profit.price_impact_bps as f64 / 100.0,
            estimated_mcap_before: 0.0, // TODO: Calculer
            estimated_mcap_after: 0.0, // TODO: Calculer
            our_position_size: profit.tokens_out_min,
            estimated_profit_pct: 0.0, // TODO: Calculer
            estimated_profit_lamports: profit.tokens_out,
            gas_cost_lamports: 0, // TODO: Calculer
            liquidity_usd: 0.0, // TODO: Calculer
            bundle_id: None,
            success: false,
            failure_reason: None,
        })
    }

    pub async fn log_transaction(&self, log: &TransactionLog) -> Result<()> {
        let log_file = "sandwich_transactions.log";
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        let log_line = format!(
            "{:?} | {} | {} | {} | {} -> {} | {} -> {} | Impact: {:.2}% | Fee: {}bps | Profit: {} SOL | Gas: {} SOL | Position: {} | Success: {}\n",
            log.timestamp,
            log.signature,
            log.dex_type,
            log.pool_id,
            log.token_in,
            log.token_out,
            log.amount_in,
            log.amount_out_min,
            log.price_impact_pct,
            log.pool_fee_bps,
            log.estimated_profit_lamports as f64 / 1e9,
            log.gas_cost_lamports as f64 / 1e9,
            log.our_position_size,
            log.success
        );

        file.write_all(log_line.as_bytes())?;
        Ok(())
    }
}