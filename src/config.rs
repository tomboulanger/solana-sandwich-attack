use solana_sdk::signature::Keypair;

// ============================================================================
// CONFIGURATION
// ============================================================================
pub struct BotConfig {
    pub rpc_url: String,
    pub ws_url: String,
    pub jito_urls: Vec<String>,
    pub keypair: Keypair,
    pub position_size_lamports: u64,
    pub min_profit_percent: f64,
    pub max_slippage_bps: u64,
    pub priority_fee_lamports: u64,
    pub jito_tip_lamports: u64,
    pub max_position_size_pct: f64,
    pub min_liquidity_usd: f64,
    // Mode test - désactive l'envoi de transactions
    pub test_mode: bool,
    pub min_mcap_usd: f64,
    pub max_mcap_usd: f64,
}

impl BotConfig {
    pub fn new() -> Self {
        // Charger les variables d'environnement - Utiliser Helius pour de meilleures performances
        let rpc_url = std::env::var("RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
        let ws_url = std::env::var("WS_URL")
            .unwrap_or_else(|_| "wss://api.mainnet-beta.solana.com".to_string());
        
        log::info!("🔧 Configuration chargée:");
        log::info!(" 📡 RPC URL: {}", rpc_url);
        log::info!(" 🌐 WS URL: {}", ws_url);
        
        Self {
            rpc_url,
            ws_url,
            
            jito_urls: vec![
                "https://mainnet.block-engine.jito.wtf/api/v1/bundles".to_string(),
                "https://amsterdam.mainnet.block-engine.jito.wtf/api/v1/bundles".to_string(),
                "https://frankfurt.mainnet.block-engine.jito.wtf/api/v1/bundles".to_string(),
                "https://ny.mainnet.block-engine.jito.wtf/api/v1/bundles".to_string(),
                "https://tokyo.mainnet.block-engine.jito.wtf/api/v1/bundles".to_string(),
            ],
    
            keypair: Keypair::from_base58_string(
                &std::env::var("PRIVATE_KEY").expect("PRIVATE_KEY env var requis")
            ),
    
            position_size_lamports: 670_000_000, // ~100$ @ 150$ SOL
            min_profit_percent: 10.0,
            max_slippage_bps: 200,
            priority_fee_lamports: 500_000,
            jito_tip_lamports: 50_000,
            max_position_size_pct: 5.0,
            min_liquidity_usd: 1_000.0, // Plus bas pour les petits tokens
            // Mode test activé par défaut
            test_mode: true,
        min_mcap_usd: 500_000.0,  // Min 500k mcap
        max_mcap_usd: 10_000_000.0, // Max 10M mcap
        }
    }
}
