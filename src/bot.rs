use crate::config::BotConfig;
use crate::dex::DexManager;
use crate::monitoring::MonitoringEngine;
use crate::sandwich::SandwichEngine;
use anyhow::Result;
use std::sync::Arc;

// ============================================================================
// MAIN BOT STRUCTURE
// ============================================================================

pub struct SandwichBot {
    pub config: Arc<BotConfig>,
    pub dex_manager: DexManager,
    pub monitoring_engine: MonitoringEngine,
    pub sandwich_engine: SandwichEngine,
}

impl SandwichBot {
    pub async fn new(config: BotConfig) -> Result<Self> {
        let config_arc = Arc::new(config);
        
        // Initialiser le gestionnaire DEX
        let config_clone = BotConfig {
            rpc_url: config_arc.rpc_url.clone(),
            ws_url: config_arc.ws_url.clone(),
            jito_urls: config_arc.jito_urls.clone(),
            keypair: config_arc.keypair.insecure_clone(),
            position_size_lamports: config_arc.position_size_lamports,
            min_profit_percent: config_arc.min_profit_percent,
            max_slippage_bps: config_arc.max_slippage_bps,
            priority_fee_lamports: config_arc.priority_fee_lamports,
            jito_tip_lamports: config_arc.jito_tip_lamports,
            max_position_size_pct: config_arc.max_position_size_pct,
            min_liquidity_usd: config_arc.min_liquidity_usd,
            test_mode: config_arc.test_mode,
            min_mcap_usd: config_arc.min_mcap_usd,
            max_mcap_usd: config_arc.max_mcap_usd,
        };
        let dex_manager = DexManager::new(config_clone).await?;
        
        // Créer les engines
        let user_token_accounts = dex_manager.user_token_accounts.clone();
        let monitoring_engine = MonitoringEngine::new(
            Arc::clone(&config_arc),
            Arc::clone(&dex_manager.rpc),
            Arc::clone(&dex_manager.async_rpc),
            Arc::clone(&dex_manager.pool_cache),
            user_token_accounts,
            Arc::clone(&dex_manager.price_cache),
        );
        
        let sandwich_engine = SandwichEngine::new(
            Arc::clone(&config_arc),
            Arc::new(monitoring_engine.clone()),
            Arc::clone(&dex_manager.rpc),
            Arc::clone(&dex_manager.async_rpc),
            dex_manager.user_token_accounts.clone(),
            config_arc.keypair.insecure_clone(),
        );

        Ok(Self {
            config: config_arc,
            dex_manager,
            monitoring_engine,
            sandwich_engine,
        })
    }

    pub async fn start(&mut self) -> Result<()> {
        // Démarrer le service de mise à jour du prix SOL
        self.monitoring_engine.start_sol_price_updater().await;
        
        // Attendre que le prix SOL soit disponible
        while !self.monitoring_engine.is_sol_price_available().await {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        // Initialiser le WebSocket pour surveiller les transactions en temps réel
        match self.monitoring_engine.initialize_websocket().await {
            Ok(_) => {
                // Démarrer le monitoring des transactions WebSocket en parallèle
                let mut monitoring_engine = self.monitoring_engine.clone_for_async();
                tokio::spawn(async move {
                    if let Err(e) = monitoring_engine.monitor_websocket_transactions().await {
                        log::error!("❌ Erreur dans monitor_websocket_transactions: {}", e);
                    }
                });
                
                // Attendre indéfiniment (le bot continue à tourner)
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
            Err(e) => {
                log::error!("❌ Erreur WebSocket: {}", e);
                return Err(e);
            }
        }
    }

}
