use crate::config::BotConfig;
use crate::types::{
    DexType, PoolInfo,
    WSOL_MINT, USDC_MINT
};
use crate::pool_parser::PoolParser;
use solana_sdk::program_pack::Pack;
use anyhow::{Result, anyhow};
use solana_client::{
    nonblocking::rpc_client::RpcClient as AsyncRpcClient,
    rpc_client::RpcClient,
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signer,
};
use tokio::time::Instant;
use spl_token::state::Account as TokenAccount;
use spl_associated_token_account::get_associated_token_address;
use std::str::FromStr;
use std::sync::Arc;
use ahash::AHashMap;

// ============================================================================
// DEX PARSING AND POOL MANAGEMENT
// ============================================================================

pub struct DexManager {
    pub config: Arc<BotConfig>,
    pub rpc: Arc<RpcClient>,
    pub async_rpc: Arc<AsyncRpcClient>,
    pub pool_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, PoolInfo>>>,
    pub user_token_accounts: AHashMap<Pubkey, Pubkey>,
    pub price_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, (f64, Instant)>>>,
    pub pool_parser: PoolParser,
}

impl DexManager {
    pub async fn new(config: BotConfig) -> Result<Self> {
        let rpc = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::processed(),
        ));

        let async_rpc = Arc::new(AsyncRpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::processed(),
        ));

        let pool_parser = PoolParser::new(Arc::clone(&async_rpc));

        let mut manager = Self {
            config: Arc::new(config),
            rpc,
            async_rpc,
            pool_cache: Arc::new(tokio::sync::RwLock::new(AHashMap::new())),
            user_token_accounts: AHashMap::new(),
            price_cache: Arc::new(tokio::sync::RwLock::new(AHashMap::new())),
            pool_parser,
        };

        manager.initialize_token_accounts().await?;
        Ok(manager)
    }

    pub async fn initialize_token_accounts(&mut self) -> Result<()> {
        log::info!("Initialisation des token accounts...");
        
        let wsol = Pubkey::from_str(WSOL_MINT)?;
        let wsol_ata = get_associated_token_address(&self.config.keypair.pubkey(), &wsol);
        self.user_token_accounts.insert(wsol, wsol_ata);

        let usdc = Pubkey::from_str(USDC_MINT)?;
        let usdc_ata = get_associated_token_address(&self.config.keypair.pubkey(), &usdc);
        self.user_token_accounts.insert(usdc, usdc_ata);

        log::info!("Token accounts initialises");
        Ok(())
    }

    pub async fn fetch_pool_info(&self, pool_id: &Pubkey, dex_type: DexType, program_id: Pubkey) -> Result<PoolInfo> {
        // Utiliser le PoolParser pour parser n'importe quel type de pool
        self.pool_parser.parse_pool(pool_id, dex_type, program_id).await
    }

    /// Récupère les informations d'un pool avec cache
    pub async fn get_pool_info_cached(&self, pool_id: &Pubkey, dex_type: DexType, program_id: Pubkey) -> Result<PoolInfo> {
        // Vérifier le cache
        let cache = self.pool_cache.read().await;
        if let Some(pool_info) = cache.get(pool_id) {
            return Ok(pool_info.clone());
        }
        drop(cache);

        // Récupérer et parser le pool
        let pool_info = self.fetch_pool_info(pool_id, dex_type, program_id).await?;

        // Mettre en cache
        let mut cache = self.pool_cache.write().await;
        cache.insert(*pool_id, pool_info.clone());

        Ok(pool_info)
    }

    /// Méthode helper pour obtenir la balance d'un token account
    pub async fn get_token_balance(&self, token_account: &Pubkey) -> Result<u64> {
        let account_data = self.async_rpc.get_account(token_account).await?;
        let token_account = TokenAccount::unpack(&account_data.data)?;
        Ok(token_account.amount)
    }

    /// Met à jour le prix SOL dans le parser
    pub fn update_sol_price(&mut self, price: f64) {
        self.pool_parser.set_sol_price(price);
    }

    /// Vérifie si un pool est valide pour le sandwich
    pub fn is_pool_valid(&self, pool: &PoolInfo, min_liquidity: f64, max_liquidity: f64) -> bool {
        self.pool_parser.is_pool_valid_for_sandwich(pool, min_liquidity, max_liquidity)
    }

    /// Calcule l'impact sur le prix d'un swap
    pub fn calculate_price_impact(&self, pool: &PoolInfo, amount_in: u64, is_a_to_b: bool) -> f64 {
        self.pool_parser.calculate_price_impact(pool, amount_in, is_a_to_b)
    }

    /// Détecte le type de DEX à partir d'une adresse de programme
    pub fn detect_dex_type(&self, program_id: &Pubkey) -> DexType {
        use crate::pool_addresses::is_known_dex_program;
        
        let program_str = program_id.to_string();
        if let Some(name) = is_known_dex_program(&program_str) {
            match name {
                n if n.contains("Raydium") => DexType::RaydiumV4,
                n if n.contains("Orca") => DexType::OrcaWhirlpool,
                n if n.contains("Meteora DLMM") => DexType::MeteoraDLMM,
                n if n.contains("Lifinity") => DexType::Lifinity,
                n if n.contains("Phoenix") => DexType::Phoenix,
                n if n.contains("Serum") => DexType::Serum,
                n if n.contains("Jupiter") => DexType::Jupiter,
                _ => {
                    log::warn!("⚠️  DEX connu mais non supporté: {} ({})", name, program_id);
                    DexType::Unsupported
                }
            }
        } else {
            log::debug!("❓ Programme DEX inconnu: {}", program_id);
            DexType::Unknown
        }
    }

    /// Analyse une pool détectée dans une transaction
    pub async fn analyze_pool_from_transaction(
        &self,
        pool_id: &Pubkey,
        program_id: &Pubkey,
    ) -> Result<PoolInfo> {
        // Détecter le type de DEX
        let dex_type = self.detect_dex_type(program_id);
        
        // Gérer les différents cas
        match dex_type {
            DexType::Unsupported => {
                log::error!("❌ POOL NON SUPPORTÉE: DEX connu mais non supporté");
                log::error!("   Pool ID: {}", pool_id);
                log::error!("   Program ID: {}", program_id);
                log::error!("   ➡️  Action: IGNORER - Type de DEX non supporté par le bot");
                return Err(anyhow!("Type de DEX non supporté: {}", program_id));
            },
            DexType::Unknown => {
                log::error!("❌ POOL INCONNUE: Programme non reconnu");
                log::error!("   Pool ID: {}", pool_id);
                log::error!("   Program ID: {}", program_id);
                log::error!("   ➡️  Action: IGNORER - Programme DEX totalement inconnu");
                return Err(anyhow!("Programme DEX inconnu: {}", program_id));
            },
            _ => {
                // DEX supporté - continuer normalement
                log::info!("✅ Pool reconnue: {:?}", dex_type);
            }
        }

        // Récupérer les informations du pool avec cache
        let pool_info = self.get_pool_info_cached(pool_id, dex_type, *program_id).await?;

        // Afficher les informations du pool
        log::info!("📊 Pool détectée: {:?}", pool_info.dex_type);
        log::info!("  💧 Liquidité: ${:.2}", pool_info.liquidity_usd);
        
        if let Some(mcap) = pool_info.market_cap_usd {
            log::info!("  📈 Market Cap: ${:.2}", mcap);
        }
        
        if let Some(price) = pool_info.token_price_usd {
            log::info!("  💵 Prix Token: ${:.8}", price);
        }

        Ok(pool_info)
    }

    pub fn get_token_account(&self, mint: &Pubkey) -> Result<Pubkey> {
        let wsol_mint = Pubkey::from_str(WSOL_MINT)?;
        let user_pubkey = self.config.keypair.pubkey();

        if *mint == wsol_mint {
            // Pour WSOL, utiliser l'ATA ou créer un compte temporaire
            Ok(get_associated_token_address(&user_pubkey, &wsol_mint))
        } else {
            self.user_token_accounts.get(mint)
                .copied()
                .ok_or_else(|| anyhow!("Token account not found for mint: {}", mint))
        }
    }
}
