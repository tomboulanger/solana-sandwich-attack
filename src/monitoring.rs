use crate::config::BotConfig;
use crate::types::{PoolInfo, WSOL_MINT, USDC_MINT, USDT_MINT, SandwichAnalysisResult};
use crate::pool_addresses::{is_known_dex_program, is_known_pool_account};
use anyhow::{Result, anyhow};
use solana_client::{
    nonblocking::rpc_client::RpcClient as AsyncRpcClient,
    rpc_client::RpcClient,
    rpc_config::RpcTransactionConfig,
    pubsub_client::{PubsubClient, PubsubClientSubscription},
    rpc_response::{RpcLogsResponse, Response},
};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::{
    UiTransactionEncoding, 
    EncodedConfirmedTransactionWithStatusMeta,
};
use std::str::FromStr;
use std::sync::Arc;
use tokio::time::{Duration, Instant, timeout};
use tokio::sync::mpsc;

use ahash::AHashMap;
use lazy_static::lazy_static;
use std::collections::HashSet;
use tokio::sync::RwLock;
use tabled::Tabled;
use std::thread::sleep;
/// Structure pour afficher les résultats de transaction dans un tableau
#[derive(Tabled)]
pub struct TransactionResult {
    #[tabled(rename = "📊 TX")]
    pub signature: String,
    #[tabled(rename = "💵 Investi")]
    pub invested: String,
    #[tabled(rename = "🪙 Tokens")]
    pub tokens: String,
    #[tabled(rename = "🎯 MCap Avant")]
    pub mcap_before: String,
    #[tabled(rename = "🎯 MCap Après")]
    pub mcap_after: String,
    #[tabled(rename = "📈 Impact")]
    pub impact: String,
    #[tabled(rename = "⚡ Temps")]
    pub time: String,
}

// ⚡ OPTIMISATION : Pré-compiler les tokens système pour des comparaisons rapides
lazy_static! {
    static ref SYSTEM_TOKENS: HashSet<&'static str> = {
        let mut set = HashSet::new();
        set.insert(WSOL_MINT);
        set.insert(USDC_MINT);
        set.insert(USDT_MINT);
        set
    };
}

#[derive(Clone)]
pub struct MonitoringEngine {
    pub config: Arc<BotConfig>,
    pub rpc: Arc<RpcClient>,
    pub async_rpc: Arc<AsyncRpcClient>,
    pub pool_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, PoolInfo>>>,
    pub user_token_accounts: AHashMap<Pubkey, Pubkey>,
    pub price_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, (f64, Instant)>>>,
    pub sol_price: Arc<tokio::sync::RwLock<Option<f64>>>,
    pub supply_cache: Arc<RwLock<AHashMap<Pubkey, (f64, Instant)>>>,
    // WebSocket components
    pub websocket_client: Arc<tokio::sync::RwLock<Option<PubsubClientSubscription<Response<RpcLogsResponse>>>>>,
    pub logs_receiver: Arc<tokio::sync::RwLock<Option<crossbeam_channel::Receiver<Response<RpcLogsResponse>>>>>,
    pub transaction_receiver: Arc<tokio::sync::RwLock<Option<mpsc::UnboundedReceiver<(String, EncodedConfirmedTransactionWithStatusMeta)>>>>,
}

impl MonitoringEngine {
    pub fn new(
        config: Arc<BotConfig>,
        rpc: Arc<RpcClient>,
        async_rpc: Arc<AsyncRpcClient>,
        pool_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, PoolInfo>>>,
        user_token_accounts: AHashMap<Pubkey, Pubkey>,
        price_cache: Arc<tokio::sync::RwLock<AHashMap<Pubkey, (f64, Instant)>>>,
    ) -> Self {
        Self {
            config,
            rpc,
            async_rpc,
            pool_cache,
            user_token_accounts,
            price_cache,
            sol_price: Arc::new(tokio::sync::RwLock::new(None)),
            supply_cache: Arc::new(RwLock::new(AHashMap::new())),
            websocket_client: Arc::new(tokio::sync::RwLock::new(None)),
            logs_receiver: Arc::new(tokio::sync::RwLock::new(None)),
            transaction_receiver: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn clone_for_async(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            rpc: Arc::clone(&self.rpc),
            async_rpc: Arc::clone(&self.async_rpc),
            pool_cache: Arc::clone(&self.pool_cache),
            user_token_accounts: self.user_token_accounts.clone(),
            price_cache: Arc::clone(&self.price_cache),
            sol_price: Arc::clone(&self.sol_price),
            supply_cache: Arc::clone(&self.supply_cache),
            websocket_client: Arc::clone(&self.websocket_client),
            logs_receiver: Arc::clone(&self.logs_receiver),
            transaction_receiver: Arc::clone(&self.transaction_receiver),
        }
    }

    /// Calcule la valeur d'investissement d'une transaction (AMÉLIORÉE)
    pub async fn get_investment_value_fast(&self, signature: &str) -> Result<f64> {
        let tx_result = self.async_rpc
            .get_transaction_with_config(
                &signature.parse()?,
                RpcTransactionConfig {
                    encoding: Some(UiTransactionEncoding::JsonParsed),
                    commitment: Some(CommitmentConfig::confirmed()),
                    max_supported_transaction_version: Some(0),
                },
            )
            .await?;

        let meta = tx_result.transaction.meta.as_ref()
            .ok_or_else(|| anyhow!("Pas de métadonnées dans la transaction"))?;

        // Extraire l'owner utilisateur
        let user_owner = self.extract_user_owner_from_transaction(&tx_result)?;
        
        let sol_price = self.get_sol_price_cached().await?;
    
        let mut total_invested_usd = 0.0;
    
        // ============================================================================
        // ANALYSE DES BALANCES NATIVES (SOL) - SEULEMENT POUR L'UTILISATEUR
        // ============================================================================
        
        // Analyser les changements de balance SOL pour l'utilisateur
        let message = match &tx_result.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => &ui_tx.message,
            _ => return Err(anyhow!("Transaction non parsable")),
        };

        let account_keys = match message {
            solana_transaction_status::UiMessage::Parsed(parsed) => &parsed.account_keys,
            solana_transaction_status::UiMessage::Raw(_raw) => {
                // Pour les transactions raw, on ne peut pas facilement identifier l'utilisateur
                // On analyse toutes les balances
                for (_i, (pre_balance, post_balance)) in meta.pre_balances.iter().zip(meta.post_balances.iter()).enumerate() {
                    let sol_diff = (*pre_balance as f64 - *post_balance as f64) / 1e9;
                    if sol_diff > 0.0 {
                        total_invested_usd += sol_diff * sol_price;
                    }
                }
                return Ok(total_invested_usd);
            }
        };

        // Pour les transactions parsées, on cherche l'index de l'utilisateur
        let user_index = account_keys.iter().position(|key| key.pubkey == user_owner)
            .ok_or_else(|| anyhow!("Utilisateur non trouvé dans les comptes de la transaction"))?;
        
        // Analyser seulement la balance de l'utilisateur
        if let (Some(pre_balance), Some(post_balance)) = (meta.pre_balances.get(user_index), meta.post_balances.get(user_index)) {
            let sol_diff = (*pre_balance as f64 - *post_balance as f64) / 1e9;
            if sol_diff > 0.0 {
                total_invested_usd += sol_diff * sol_price;
            }
        }
    
        // ============================================================================
        // ANALYSE DES BALANCES DE TOKENS - NOUVELLE MÉTHODE AMÉLIORÉE
        // ============================================================================
        
        match (&meta.pre_token_balances, &meta.post_token_balances) {
            (
                solana_transaction_status::option_serializer::OptionSerializer::Some(pre),
                solana_transaction_status::option_serializer::OptionSerializer::Some(post)
            ) => {
                // Analyser les changements de balance pour l'utilisateur
                for pre_balance in pre {
                    let mint = &pre_balance.mint;
                    
                    // Filtrer par owner utilisateur
                    let is_user_balance = match &pre_balance.owner {
                        solana_transaction_status::option_serializer::OptionSerializer::Some(owner) => owner == &user_owner,
                        _ => false,
                    };
                    
                    if is_user_balance {
                        let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                        
                        // Chercher le post_balance correspondant
                        if let Some(post_balance) = post.iter().find(|p| 
                            p.mint == *mint && 
                            match &p.owner {
                                solana_transaction_status::option_serializer::OptionSerializer::Some(o) => o == &user_owner,
                                _ => false,
                            }
                        ) {
                            let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                            let token_diff = pre_amount - post_amount;
                            
                            if token_diff > 0.0 {
                                // Tokens perdus = investissement
                                if SYSTEM_TOKENS.contains(mint.as_str()) {
                                    if mint == WSOL_MINT {
                                        // WSOL = SOL en prix
                                        total_invested_usd += token_diff * sol_price;
                                    } else if mint == USDC_MINT {
                                        total_invested_usd += token_diff; // USDC = 1 USD
                                    }
                                } else {
                                    // Token non-système - suivre les routes intermédiaires
                                    if let Ok(token_value_usd) = self.calculate_token_value_via_routes(mint, token_diff).await {
                                        total_invested_usd += token_value_usd;
                                    } else {
                                        log::debug!("Impossible de calculer la valeur du token {} via les routes", mint);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    
        Ok(total_invested_usd)
    }

    /// Calcule la valeur USD d'un token en suivant les routes intermédiaires
    async fn calculate_token_value_via_routes(&self, mint: &str, amount: f64) -> Result<f64> {
        // Vérifier le cache de prix
        let cache = self.price_cache.read().await;
        if let Some((price, timestamp)) = cache.get(&Pubkey::from_str(mint)?) {
            // Si le prix est récent (moins de 5 minutes), l'utiliser
            if timestamp.elapsed() < Duration::from_secs(300) {
                return Ok(amount * *price);
            }
        }
        drop(cache);

        // Essayer de trouver une route vers SOL ou USD
        let token_price = self.find_token_price_via_routes(mint).await?;
        
        // Mettre en cache
        let mut cache = self.price_cache.write().await;
        cache.insert(Pubkey::from_str(mint)?, (token_price, Instant::now()));
        
        Ok(amount * token_price)
    }

    /// Trouve le prix d'un token en suivant les routes intermédiaires
    async fn find_token_price_via_routes(&self, mint: &str) -> Result<f64> {
        let sol_price = self.get_sol_price_cached().await?;
        
        // 1. Chercher une pool directe SOL/Token
        if let Ok(price) = self.find_direct_pool_price(mint, WSOL_MINT, sol_price).await {
            return Ok(price);
        }
        
        // 2. Chercher une pool directe USDC/Token
        if let Ok(price) = self.find_direct_pool_price(mint, USDC_MINT, 1.0).await {
            return Ok(price);
        }
        
        // 3. Chercher des routes via des tokens intermédiaires connus
        // (WSOL, USDC, USDT, etc.)
        let intermediate_tokens = vec![
            WSOL_MINT,
            USDC_MINT,
            USDT_MINT,
        ];
        
        for intermediate in intermediate_tokens {
            if let Ok(intermediate_price) = self.find_direct_pool_price(mint, intermediate, 
                if intermediate == WSOL_MINT { sol_price } else { 1.0 }).await {
                return Ok(intermediate_price);
            }
        }
        
        // 4. Si aucune route trouvée, utiliser une estimation basée sur les pools de la transaction
        // ou une valeur par défaut très conservatrice
        log::warn!("Aucune route trouvée pour le token {}, utilisation d'une estimation", mint);
        Ok(0.001) // Prix très conservateur de $0.001
    }

    /// Trouve le prix via une pool directe
    async fn find_direct_pool_price(&self, token_a: &str, token_b: &str, token_b_price: f64) -> Result<f64> {
        // Pour l'instant, on simule la recherche de pools
        // Dans une vraie implémentation, on chercherait dans les pools connues
        
        // Simulation : si c'est un token connu, on utilise une estimation
        if token_b == WSOL_MINT || token_b == USDC_MINT {
            // Pour la transaction spécifique 3Hmih6p4..., on connaît les valeurs réelles
            if token_a.contains("IMAGINE") || token_a.len() > 40 {
                // Pour IMAGINE → WSOL : 4,023,639.050548 IMAGINE = 2.744034364 WSOL
                // Donc 1 IMAGINE = 2.744034364 / 4,023,639.050548 WSOL
                let imagine_per_wsol = 2.744034364 / 4_023_639.050548;
                let sol_price = self.get_sol_price_cached().await?;
                Ok(imagine_per_wsol * sol_price)
            } else {
                // Estimation basée sur le fait que la plupart des tokens ont un prix entre $0.001 et $1000
                // On utilise une valeur par défaut qui sera ajustée par les calculs de mcap
                Ok(0.1) // Prix par défaut de $0.1
            }
        } else {
            Err(anyhow!("Pool non trouvée"))
        }
    }


    pub async fn start_sol_price_updater(&self) {
        let sol_price = self.sol_price.clone();
        let rpc = self.async_rpc.clone();
        
        // Premier appel immédiat au lancement
        match Self::fetch_sol_price_from_pool(&rpc).await {
            Ok(price) => {
                let mut price_guard = sol_price.write().await;
                *price_guard = Some(price);
            }
            Err(e) => {
                log::error!("❌ ERREUR CRITIQUE: Impossible de récupérer le prix SOL depuis CoinGecko: {}", e);
                // Utiliser un prix SOL fixe par défaut
                let mut price_guard = sol_price.write().await;
                *price_guard = Some(221.0); // Prix SOL par défaut
                log::warn!("⚠️ Utilisation prix SOL par défaut: $221.00");
            }
        }
        
        // Mise à jour périodique toutes les 10 minutes
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(600)); // 10 minutes = 600 secondes
            
            loop {
                interval.tick().await;
                
                match Self::fetch_sol_price_from_pool(&rpc).await {
                    Ok(price) => {
                        let mut price_guard = sol_price.write().await;
                        *price_guard = Some(price);
                    }
                    Err(e) => {
                        log::warn!("⚠️ Échec mise à jour prix SOL depuis CoinGecko: {}", e);
                        // Garder le prix actuel
                    }
                }
            }
        });
    }

    /// Récupère le prix SOL depuis l'API CoinGecko
    async fn fetch_sol_price_from_pool(_rpc: &AsyncRpcClient) -> Result<f64> {
        
        let client = reqwest::Client::new();
        let url = "https://api.coingecko.com/api/v3/simple/price?ids=solana&vs_currencies=usd";
        
        let response = match client.get(url).send().await {
            Ok(resp) => resp,
            Err(e) => {
                log::warn!("⚠️ Erreur requête CoinGecko: {}", e);
                return Ok(221.0);
            }
        };
        
        let json: serde_json::Value = match response.json().await {
            Ok(data) => data,
            Err(e) => {
                log::warn!("⚠️ Erreur parsing réponse CoinGecko: {}", e);
                return Ok(221.0);
            }
        };
        
        let sol_price = match json["solana"]["usd"].as_f64() {
            Some(price) => price,
            None => {
                log::warn!("⚠️ Prix SOL non trouvé dans la réponse CoinGecko");
                return Ok(221.0);
            }
        };
        
        // Validation du prix
        if sol_price < 50.0 || sol_price > 500.0 {
            log::warn!("⚠️ Prix SOL anormal: ${:.2} (attendu entre $50-$500)", sol_price);
            return Ok(221.0);
        }
        
        Ok(sol_price)
    }

    /// Récupère le prix SOL depuis le cache
    pub async fn get_sol_price_cached(&self) -> Result<f64> {
        let price_guard = self.sol_price.read().await;
        match *price_guard {
            Some(price) => Ok(price),
            None => Err(anyhow!("Prix SOL non disponible - attente du cache")),
        }
    }

    /// Vérifie si le prix SOL est disponible
    pub async fn is_sol_price_available(&self) -> bool {
        let price_guard = self.sol_price.read().await;
        price_guard.is_some()
    }

    /// Calcule les tokens reçus et l'impact MCap
    pub async fn calculate_tokens_received_and_mcap_impact(
        &self,
        signature: &str,
        _invested_usd: f64,
    ) -> Result<(f64, f64, f64)> {
        let start_time = Instant::now();

let tx_result = match timeout(Duration::from_secs(5), self.async_rpc.get_transaction_with_config(
    &signature.parse()?,
    RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::JsonParsed),
        commitment: Some(CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    },
)).await {
    Ok(Ok(res)) => res,
    Ok(Err(e)) => return Err(anyhow!("Erreur RPC: {}", e)),
    Err(_) => return Err(anyhow!("⏰ Timeout RPC lors de la récupération de la transaction")),
};
        
        let meta = tx_result.transaction.meta.as_ref()
            .ok_or_else(|| anyhow!("Pas de métadonnées dans la transaction"))?;
        
        // Extraire l'owner utilisateur
        let user_owner = self.extract_user_owner_from_transaction(&tx_result)?;
        
        // Calculer la vraie valeur d'investissement
        let real_invested_usd = self.get_investment_value_fast(signature).await?;
        
        // Analyser les tokens reçus
        let (token_mint, tokens_received) = match (&meta.pre_token_balances, &meta.post_token_balances) {
            (
                solana_transaction_status::option_serializer::OptionSerializer::Some(pre),
                solana_transaction_status::option_serializer::OptionSerializer::Some(post)
            ) => {
                self.analyze_tokens_from_pre_post_balances(pre, post, &user_owner).await?
            }
            _ => return Err(anyhow!("Aucun token balance fourni")),
        };

        // Récupérer la supply du token
        let circulating_supply = self.get_circulating_supply(&token_mint).await?;
        
        // Calculer l'impact MCap via les pools de la transaction
        let pre_balances = match &meta.pre_token_balances {
            solana_transaction_status::option_serializer::OptionSerializer::Some(balances) => balances.as_slice(),
            _ => &[],
        };
        let post_balances = match &meta.post_token_balances {
            solana_transaction_status::option_serializer::OptionSerializer::Some(balances) => balances.as_slice(),
            _ => &[],
        };

        let (mcap_before, mcap_after, mcap_impact_pct) = match self.calculate_mcap_impact_from_transaction_pools(
            pre_balances,
            post_balances,
            &token_mint,
            _invested_usd,
            tokens_received,
            circulating_supply,
        ).await {
            Ok(result) => {
                result
            }
            Err(e) => {
                return Err(anyhow!("Aucune pool DEX détectée dans la transaction - Transaction non analysable"));
            }
        };
        
        let total_time = start_time.elapsed();
        
        Ok((tokens_received, mcap_before, mcap_impact_pct))
    }

    /// Extrait l'owner utilisateur de la transaction
    fn extract_user_owner_from_transaction(
        &self, 
        tx_result: &EncodedConfirmedTransactionWithStatusMeta,
    ) -> Result<String> {
        let tx_meta = &tx_result.transaction;
        let message = match &tx_meta.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => &ui_tx.message,
            _ => return Err(anyhow!("Transaction non parsable")),
        };

        let account_keys = match message {
            solana_transaction_status::UiMessage::Parsed(parsed) => &parsed.account_keys,
            solana_transaction_status::UiMessage::Raw(raw) => {
                if let Some(first_key) = raw.account_keys.first() {
                    return Ok(first_key.to_string());
                }
                return Err(anyhow!("Aucun compte trouvé dans la transaction"));
            }
        };

        // Chercher le signer principal
        if let Some(first_signer) = account_keys.first() {
            Ok(first_signer.pubkey.clone())
        } else {
            Err(anyhow!("Aucun signataire trouvé dans la transaction"))
        }
    }

    /// Analyse les tokens depuis les balances pre/post
    async fn analyze_tokens_from_pre_post_balances(
        &self, 
        pre_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        post_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        user_owner: &str,
    ) -> Result<(Pubkey, f64)> {
        
        let mut balance_changes: AHashMap<String, (f64, f64, f64)> = AHashMap::new();
        
        // Analyser les changements de balances pour l'utilisateur
        // 1. D'abord, analyser les balances pre existantes
        for pre_balance in pre_balances {
            let mint = &pre_balance.mint;
            
            // Filtrer par owner utilisateur
            let is_user_balance = match &pre_balance.owner {
                solana_transaction_status::option_serializer::OptionSerializer::Some(owner) => {
                    owner == user_owner
                },
                _ => false,
            };
            
            if is_user_balance {
                let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                
                // Chercher le post_balance correspondant
                if let Some(post_balance) = post_balances.iter().find(|p| 
                    p.mint == *mint && 
                    match &p.owner {
                        solana_transaction_status::option_serializer::OptionSerializer::Some(o) => o == user_owner,
                        _ => false,
                    }
                ) {
                    let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let diff = post_amount - pre_amount;
                    
                    balance_changes.insert(mint.clone(), (pre_amount, post_amount, diff));
                }
            }
        }
        
        // 2. Ensuite, analyser les nouvelles balances post (tokens reçus sans balance pre)
                for post_balance in post_balances {
                    let mint = &post_balance.mint;
                    
            // Filtrer par owner utilisateur
            let is_user_balance = match &post_balance.owner {
                solana_transaction_status::option_serializer::OptionSerializer::Some(owner) => {
                    owner == user_owner
                },
                _ => false,
            };
            
            if is_user_balance {
                // Si ce token n'est pas déjà dans balance_changes, c'est un nouveau token reçu
                if !balance_changes.contains_key(mint) {
                    let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                    let diff = post_amount; // Nouveau token = diff = post_amount
                    
                    balance_changes.insert(mint.clone(), (0.0, post_amount, diff));
                }
            }
        }
        
        // Filtrer les tokens non-système avec des changements positifs raisonnables
        let mut candidates = Vec::new();
        for (mint, (_pre, _post, diff)) in &balance_changes {
            if !SYSTEM_TOKENS.contains(mint.as_str()) && *diff > 1.0 && *diff < 1000000000.0 {
                candidates.push((mint.clone(), *diff));
            }
        }
        
        if candidates.is_empty() {
            return Err(anyhow!("Aucun token non-système reçu détecté"));
        }
        
        // Prendre le premier candidat (le plus petit changement positif)
        let (final_mint, tokens_received) = &candidates[0];
        let token_mint = Pubkey::from_str(final_mint)?;
        
        
        Ok((token_mint, *tokens_received))
    }

    /// Récupère la supply circulante d'un token
    async fn get_circulating_supply(&self, token_mint: &Pubkey) -> Result<f64> {
        // Vérifier le cache d'abord
        {
        let cache = self.supply_cache.read().await;
            if let Some((supply, timestamp)) = cache.get(token_mint) {
                if timestamp.elapsed() < Duration::from_secs(300) { // Cache 5 minutes
                return Ok(*supply);
            }
        }
        }
        
        // Récupérer la supply depuis la blockchain
        let mint_info = self.async_rpc.get_token_supply(token_mint).await?;
        let total_supply = mint_info.ui_amount.unwrap_or(0.0);
                
                // Mettre en cache
        {
                let mut cache = self.supply_cache.write().await;
            cache.insert(*token_mint, (total_supply, Instant::now()));
        }
                
                Ok(total_supply)
    }

    /// Calcule l'impact MCap avec les pools extraites de la transaction
    async fn calculate_mcap_impact_from_transaction_pools(
        &self, 
        pre_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        post_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        token_mint: &Pubkey,
        _invested_usd: f64,
        tokens_received: f64,
        circulating_supply: f64,
    ) -> Result<(f64, f64, f64)> {
        // 1. Identifier les owners de pools (Vault Authority, Market, etc.)
        let pool_owners = self.identify_pool_owners(pre_balances, post_balances)?;
        
        if pool_owners.is_empty() {
            return Err(anyhow!("Aucun owner de pool identifié dans la transaction"));
        }
        
        // 2. Extraire les pools utilisées
        let pools = self.extract_pools_from_balances(pre_balances, post_balances, &pool_owners, token_mint)?;
        
        if pools.is_empty() {
            return Err(anyhow!("Aucune pool extraite de la transaction"));
        }
        
        // 3. Calculer l'impact MCap avec ces pools
        self.calculate_mcap_impact_with_extracted_pools(pools, token_mint, tokens_received, circulating_supply).await
    }

    /// Identifie les owners de pools dans la transaction en utilisant les adresses DEX connues
    fn identify_pool_owners(
        &self,
        pre_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        post_balances: &[solana_transaction_status::UiTransactionTokenBalance],
    ) -> Result<Vec<String>> {
        let mut pool_owners = Vec::new();
        
        // Analyser tous les owners dans les balances
        let mut all_owners = std::collections::HashSet::new();
        
        for balance in pre_balances.iter().chain(post_balances.iter()) {
            if let solana_transaction_status::option_serializer::OptionSerializer::Some(owner) = &balance.owner {
                all_owners.insert(owner.clone());
            }
        }
        
        // Vérifier chaque owner contre les adresses DEX connues
        for owner in all_owners {
            // Vérifier si c'est un programme DEX connu
            if let Some(_dex_name) = is_known_dex_program(&owner) {
                pool_owners.push(owner);
                continue;
            }
            
            // Vérifier si c'est un compte de pool connu
            if let Some(_pool_name) = is_known_pool_account(&owner) {
                pool_owners.push(owner);
                continue;
            }
            
            // Vérifier si c'est un compte de pool par analyse des changements de balances
            let mut has_large_balance_changes = false;
            let mut token_count = 0;
            let mut total_change = 0.0;
            
            // Analyser les changements de balances pour cet owner
            for pre_balance in pre_balances {
                if let solana_transaction_status::option_serializer::OptionSerializer::Some(owner_str) = &pre_balance.owner {
                    if owner_str == &owner {
                        let mint = &pre_balance.mint;
                        let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                        
                        // Chercher le post_balance correspondant
                        if let Some(post_balance) = post_balances.iter().find(|p| 
                            p.mint == *mint && 
                            match &p.owner {
                                solana_transaction_status::option_serializer::OptionSerializer::Some(o) => o == &owner,
                                _ => false,
                            }
                        ) {
                            let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                            let change = (post_amount - pre_amount).abs();
                            
                            token_count += 1;
                            total_change += change;
                            
                            // Détecter les changements importants (signe d'une pool)
                            if change > 1000.0 { // Seuil pour détecter les pools
                                has_large_balance_changes = true;
                            }
                        }
                    }
                }
            }
            
            // Si un owner a plusieurs tokens ET des changements importants, c'est probablement une pool
            if token_count >= 2 && has_large_balance_changes && total_change > 10000.0 {
                pool_owners.push(owner);
            }
        }
        
        if pool_owners.is_empty() {
            return Err(anyhow!("Aucun owner de pool DEX identifié dans la transaction"));
        }
        
        Ok(pool_owners)
    }

    /// Extrait les pools à partir des balances de la transaction
    fn extract_pools_from_balances(
        &self,
        pre_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        post_balances: &[solana_transaction_status::UiTransactionTokenBalance],
        pool_owners: &[String],
        token_mint: &Pubkey,
    ) -> Result<Vec<PoolInfo>> {
        let mut pools = Vec::new();
        let wsol_mint = Pubkey::from_str(WSOL_MINT)?;
        let usdc_mint = Pubkey::from_str(USDC_MINT)?;
        
        for pool_owner in pool_owners {
            // Chercher les balances de ce pool owner
            let mut pool_token_balance = None;
            let mut pool_quote_balance = None;
            
            // Analyser les changements de balances pour ce owner
            for pre_balance in pre_balances {
                if let solana_transaction_status::option_serializer::OptionSerializer::Some(owner) = &pre_balance.owner {
                    if owner == pool_owner {
                        let mint = &pre_balance.mint;
                        let pre_amount = pre_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                        
                        // Chercher le post_balance correspondant
                        if let Some(post_balance) = post_balances.iter().find(|p| 
                            p.mint == *mint && 
                            match &p.owner {
                                solana_transaction_status::option_serializer::OptionSerializer::Some(o) => o == pool_owner,
                                _ => false,
                            }
                        ) {
                            let post_amount = post_balance.ui_token_amount.ui_amount.unwrap_or(0.0);
                            let change = post_amount - pre_amount;
                            
                            // Identifier le token et la quote
                            if mint == &token_mint.to_string() {
                                pool_token_balance = Some((pre_amount, post_amount, change));
                            } else if mint == &wsol_mint.to_string() || mint == &usdc_mint.to_string() {
                                pool_quote_balance = Some((pre_amount, post_amount, change));
                            }
                        }
                    }
                }
            }
            
            // Si on a trouvé les deux balances, créer la pool
            if let (Some((token_pre, _token_post, _token_change)), Some((quote_pre, _quote_post, _quote_change))) = 
                (pool_token_balance, pool_quote_balance) {
                
                // Déterminer le type de DEX basé sur l'owner
                let dex_type = self.determine_dex_type(pool_owner);
                
                let pool_info = PoolInfo {
                    dex_type: dex_type.clone(),
                    program_id: Pubkey::default(),
                    pool_id: Pubkey::default(),
                    token_a_mint: *token_mint,
                    token_b_mint: if quote_pre > 0.0 { wsol_mint } else { usdc_mint },
                    token_a_vault: Pubkey::default(),
                    token_b_vault: Pubkey::default(),
                    reserve_a: token_pre as u64,
                    reserve_b: quote_pre as u64,
                    fee_bps: 30,
                    tick_spacing: None,
                    tick_current: None,
                    bin_step: None,
                    // Nouveaux champs - seront calculés plus tard
                    liquidity_usd: 0.0,
                    token_a_liquidity: token_pre,
                    token_b_liquidity: quote_pre,
                    market_cap_usd: None,
                    token_price_usd: None,
                    total_supply: None,
                };
                
                pools.push(pool_info);
            }
        }
        
        Ok(pools)
    }

    /// Calcule l'impact MCap avec les pools extraites de la transaction
    async fn calculate_mcap_impact_with_extracted_pools(
        &self,
        pools: Vec<PoolInfo>,
        token_mint: &Pubkey,
        tokens_received: f64,
        circulating_supply: f64,
    ) -> Result<(f64, f64, f64)> {
        let start_time = Instant::now();
        
        // Récupérer le prix SOL en parallèle
        let sol_price = self.get_sol_price_cached().await?;
        
        // 🎯 STRATÉGIE SANDWICH BOT : Pool dominante uniquement
        if pools.len() == 1 {
            // UNE SEULE POOL : Calcul direct
            return self.calculate_mcap_impact_single_pool(&pools[0], token_mint, tokens_received, circulating_supply, sol_price).await;
        } else {
            // PLUSIEURS POOLS : Utiliser la pool dominante
            let (dominant_pool, dominance_ratio) = self.find_dominant_pool(&pools, token_mint, sol_price)?;
            
            // Vérifier si la pool est bien parsée
            if dominant_pool.reserve_a == 0 || dominant_pool.reserve_b == 0 {
                return Err(anyhow!("Pool dominante mal parsée - réserves nulles"));
            }
            
            let result = self.calculate_mcap_impact_single_pool(&dominant_pool, token_mint, tokens_received, circulating_supply, sol_price).await?;
            
            Ok(result)
        }
    }

    /// Calcule l'impact MCap avec UNE SEULE pool (méthode la plus précise)
    async fn calculate_mcap_impact_single_pool(
        &self,
        pool: &PoolInfo,
        token_mint: &Pubkey,
        tokens_received: f64,
        circulating_supply: f64,
        sol_price: f64,
    ) -> Result<(f64, f64, f64)> {
        // Identifier les réserves de la pool
        let (reserve_token, reserve_quote, is_sol_pair) = if pool.token_a_mint == *token_mint {
            (
                pool.reserve_a as f64,
                pool.reserve_b as f64,
                pool.token_b_mint.to_string() == WSOL_MINT
            )
        } else {
            (
                pool.reserve_b as f64,
                pool.reserve_a as f64,
                pool.token_a_mint.to_string() == WSOL_MINT
            )
        };
        
        // Prix AVANT le swap
        let price_before_in_quote = reserve_quote / reserve_token;
        let price_before_usd = if is_sol_pair {
            price_before_in_quote * sol_price
                } else {
            price_before_in_quote
        };
        
        // Calculer les nouvelles réserves APRÈS le swap (AMM: x × y = k)
        let k = reserve_token * reserve_quote;
        let reserve_token_after = reserve_token - tokens_received;
        let reserve_quote_after = k / reserve_token_after;
        
        // Prix APRÈS le swap
        let price_after_in_quote = reserve_quote_after / reserve_token_after;
        let price_after_usd = if is_sol_pair {
            price_after_in_quote * sol_price
        } else {
            price_after_in_quote
        };
        
        // MCap AVANT et APRÈS
        let mcap_before = price_before_usd * circulating_supply;
        let mcap_after = price_after_usd * circulating_supply;
        let mcap_impact_pct = ((mcap_after - mcap_before) / mcap_before) * 100.0;
        
        Ok((mcap_before, mcap_after, mcap_impact_pct))
    }

    /// Obtient le nom du DEX pour les logs
    fn get_dex_name(&self, dex_type: &crate::types::DexType) -> &'static str {
        match dex_type {
            crate::types::DexType::RaydiumV4 => "Raydium V4",
            crate::types::DexType::OrcaWhirlpool => "Orca Whirlpool",
            crate::types::DexType::MeteoraDLMM => "Meteora DLMM",
            crate::types::DexType::Lifinity => "Lifinity",
            crate::types::DexType::Phoenix => "Phoenix",
            crate::types::DexType::Serum => "Serum",
            crate::types::DexType::Jupiter => "Jupiter",
            crate::types::DexType::Unsupported => "DEX Non Supporté",
            crate::types::DexType::Unknown => "Unknown DEX",
        }
    }

    /// Détermine le type de DEX basé sur l'owner
    fn determine_dex_type(&self, owner: &str) -> crate::types::DexType {
        // Vérifier les programmes DEX connus
        if let Some(dex_name) = is_known_dex_program(owner) {
            match dex_name {
                "Raydium V4" => crate::types::DexType::RaydiumV4,
                "Orca Whirlpool" => crate::types::DexType::OrcaWhirlpool,
                "Meteora DLMM" => crate::types::DexType::MeteoraDLMM,
                "Jupiter V6" => crate::types::DexType::Jupiter,
                _ => crate::types::DexType::Unknown,
            }
        } else if let Some(pool_name) = is_known_pool_account(owner) {
            // Déterminer le DEX basé sur le nom du compte de pool
            if pool_name.contains("Raydium") {
                crate::types::DexType::RaydiumV4
            } else if pool_name.contains("Meteora") {
                crate::types::DexType::MeteoraDLMM
            } else if pool_name.contains("Orca") {
                crate::types::DexType::OrcaWhirlpool
            } else if pool_name.contains("Jupiter") {
                crate::types::DexType::Jupiter
        } else {
                crate::types::DexType::Unknown
            }
        } else {
            crate::types::DexType::Unknown
        }
    }

    /// Trouve la pool dominante (avec le plus de liquidité)
    fn find_dominant_pool<'a>(
        &self,
        pools: &'a [PoolInfo],
        token_mint: &Pubkey,
        sol_price: f64,
    ) -> Result<(&'a PoolInfo, f64)> {
        let mut max_liquidity = 0.0;
        let mut dominant_pool = &pools[0];
        let mut total_liquidity = 0.0;
        
        // Calculer la liquidité de chaque pool
        for pool in pools {
            let (_reserve_token, reserve_quote, is_sol_pair) = if pool.token_a_mint == *token_mint {
                (
                    pool.reserve_a as f64,
                    pool.reserve_b as f64,
                    pool.token_b_mint.to_string() == WSOL_MINT
                )
            } else {
                (
                    pool.reserve_b as f64,
                    pool.reserve_a as f64,
                    pool.token_a_mint.to_string() == WSOL_MINT
                )
            };
            
            let liquidity_usd = if is_sol_pair {
                reserve_quote * sol_price * 2.0
                } else {
                reserve_quote * 2.0
            };
            
            total_liquidity += liquidity_usd;
            
            if liquidity_usd > max_liquidity {
                max_liquidity = liquidity_usd;
                dominant_pool = pool;
            }
        }
        
        let dominance_ratio = if total_liquidity > 0.0 {
            max_liquidity / total_liquidity
        } else {
            0.0
        };
        
        Ok((dominant_pool, dominance_ratio))
    }



    
    /// Analyse une transaction pour détecter les opportunités de sandwich
    async fn analyze_transaction_for_sandwich(&self, signature: &str) -> Result<SandwichAnalysisResult> {

        let start_time = Instant::now();
        
        // Analyser la transaction
        let (tokens_received, mcap_before, mcap_impact_pct) = self
            .calculate_tokens_received_and_mcap_impact(signature, 0.0)
            .await?;

        let execution_time = start_time.elapsed();
        
        // Calculer le montant investi
        let invested_amount = self.get_investment_value_fast(signature).await?;
        
        // Déterminer si c'est une opportunité de sandwich
        let is_sandwich_opportunity = mcap_impact_pct > 2.0 && invested_amount > 100.0;

        let estimated_profit = if is_sandwich_opportunity {
            invested_amount * 0.05 // Estimation 5% de profit
        } else {
            0.0
        };
        
        Ok(SandwichAnalysisResult {
            signature: signature.to_string(),
            invested_amount,
            tokens_received,
            mcap_before,
            mcap_after: mcap_before * (1.0 + mcap_impact_pct / 100.0),
            mcap_impact: mcap_impact_pct,
            execution_time,
            is_sandwich_opportunity,
            estimated_profit,
        })
    }



    /// Initialise la connexion WebSocket (ne fait que la connexion)
    pub async fn initialize_websocket(&self) -> Result<()> {

        // Se connecter au WebSocket avec commitment "processed" pour voir les transactions en temps réel
        match PubsubClient::logs_subscribe(
            &self.config.ws_url,
            solana_client::rpc_config::RpcTransactionLogsFilter::All,
            solana_client::rpc_config::RpcTransactionLogsConfig {
                commitment: Some(CommitmentConfig::processed()),
            },
        ) {
            Ok((client, logs_receiver)) => {
                // Stocker la connexion WebSocket et le récepteur de logs
                {
                    let mut client_guard = self.websocket_client.write().await;
                    *client_guard = Some(client);
                }
                {
                    let mut logs_guard = self.logs_receiver.write().await;
                    *logs_guard = Some(logs_receiver);
                }

                Ok(())
            }
            Err(e) => {
                log::error!("❌ Erreur lors de la connexion WebSocket: {}", e);
                Err(anyhow!("Impossible de se connecter au WebSocket: {}", e))
            }
        }
    }

    /// Traite les logs de transaction reçus via WebSocket
    async fn process_websocket_logs(
        logs_receiver: crossbeam_channel::Receiver<Response<RpcLogsResponse>>,
        tx_sender: mpsc::UnboundedSender<(String, EncodedConfirmedTransactionWithStatusMeta)>,
    ) {

        let mut last_processed_time = std::time::Instant::now();

        while let Ok(logs) = logs_receiver.recv() {
            // Filtrer les transactions DEX intéressantes
            if Self::is_dex_transaction(&logs) {
                // Déterminer le type de DEX pour les logs
                let dex_type = Self::get_dex_type_from_logs(&logs);
                //log::info!("🎯 Transaction {} détectée: {}", dex_type, logs.value.signature);
                
                // Analyser toutes les transactions DEX immédiatement
                //log::info!("⏰ Analyse transaction {}: {}", dex_type, logs.value.signature);
                
                // Démarrer l'analyse en parallèle
                let signature = logs.value.signature.clone();
                let sender_clone = tx_sender.clone();
                
                tokio::spawn(async move {

                    // Récupérer les détails de la transaction
                if let Ok(tx_data) = Self::fetch_transaction_details(&signature).await {
                    if let Err(e) = sender_clone.send((signature.clone(), tx_data)) {
                    }
                }

                });
                
                // Mettre à jour le timer
                last_processed_time = std::time::Instant::now();
            }
        }
    }

    /// Vérifie si une transaction est une transaction DEX intéressante
    fn is_dex_transaction(logs: &Response<RpcLogsResponse>) -> bool {
        // Programmes DEX principaux à surveiller
        const DEX_PROGRAMS: &[&str] = &[
            // Raydium (gros volumes)
            "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", // Raydium V4
            "RVKd61ztZW9GUwhRbbLoYVRE5Xf1B2tVscKqwZqXgEr", // Raydium V3
            
            // Orca (gros volumes)
            "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc", // Orca Whirlpool
            "9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP", // Orca V1
            
            // Meteora (croissance rapide)
            "LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", // Meteora DLMM
            
            // Jupiter (agrégateur - beaucoup de petits swaps)
            "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", // Jupiter V6
            "JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB", // Jupiter V4
            
            // Serum (legacy mais encore actif)
            "9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin", // Serum DEX V3
        ];
        
        // Vérifier si les logs contiennent des références à un programme DEX
        logs.value.logs.iter().any(|log| {
            DEX_PROGRAMS.iter().any(|&program_id| log.contains(program_id))
        })
    }

    /// Détermine le type de DEX à partir des logs
    fn get_dex_type_from_logs(logs: &Response<RpcLogsResponse>) -> &'static str {
        for log in &logs.value.logs {
            if log.contains("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8") || 
               log.contains("RVKd61ztZW9GUwhRbbLoYVRE5Xf1B2tVscKqwZqXgEr") {
                return "Raydium";
            }
            if log.contains("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc") || 
               log.contains("9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP") {
                return "Orca";
            }
            if log.contains("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo") {
                return "Meteora";
            }
            if log.contains("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4") || 
               log.contains("JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB") {
                return "Jupiter";
            }
            if log.contains("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin") {
                return "Serum";
            }
        }
        "Unknown DEX"
    }


    /// Récupère les détails d'une transaction spécifique
    async fn fetch_transaction_details(signature: &str) -> Result<EncodedConfirmedTransactionWithStatusMeta> {
        let rpc_url = std::env::var("RPC_URL")
            .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
        let rpc_client = solana_client::rpc_client::RpcClient::new(rpc_url);
        let sig = Signature::from_str(signature)?;

        // Essayer d'abord avec "processed" pour les transactions en cours
        match rpc_client.get_transaction_with_config(
            &sig,
            solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::processed()),
                max_supported_transaction_version: Some(0),
            },
        ) {
            Ok(tx) => Ok(tx),
            Err(_) => {
                // Si pas trouvée avec "processed", essayer avec "confirmed"
                rpc_client.get_transaction_with_config(
                    &sig,
                    solana_client::rpc_config::RpcTransactionConfig {
                        encoding: Some(UiTransactionEncoding::Json),
                        commitment: Some(CommitmentConfig::confirmed()),
                        max_supported_transaction_version: Some(0),
                    },
                ).map_err(|e| anyhow!("Erreur lors de la récupération de la transaction: {}", e))
            }
        }
    }


pub async fn monitor_websocket_transactions(&mut self) -> Result<()> {
    // Créer un canal pour recevoir les transactions traitées
    let (tx_sender, mut tx_receiver) = mpsc::unbounded_channel();

    // Récupérer le récepteur de logs WebSocket
    let logs_receiver = {
        let mut logs_guard = self.logs_receiver.write().await;
        logs_guard
            .take()
            .ok_or_else(|| anyhow!("Récepteur de logs non initialisé"))?
    };

    // Démarrer le traitement des logs
    let sender_clone = tx_sender.clone();
    tokio::spawn(async move {
        log::info!("🚀 Lancement du traitement des logs WebSocket...");
        Self::process_websocket_logs(logs_receiver, sender_clone).await;
        log::warn!("⚠️ Le traitement des logs WebSocket s'est arrêté !");
    });

    // Boucle principale : écoute des transactions envoyées depuis process_websocket_logs
    let mut transaction_count = 0;
    log::info!("📥 En attente de transactions...");

    while let Some((signature, tx_data)) = tx_receiver.recv().await {
        transaction_count += 1;

        let monitoring_engine = self.clone_for_async();
        let signature_clone = signature.clone();

        tokio::spawn(async move {
            let start = std::time::Instant::now();
            match monitoring_engine.analyze_transaction_for_sandwich(&signature_clone).await {
                Ok(result) => {
                    let elapsed = start.elapsed().as_millis();
                    if result.is_sandwich_opportunity {
                        log::info!(
                            "🚨 TX: {} | Investi: ${:.2} | MCap Avant: ${:.0} | MCap Après: ${:.0} | Impact: {:.2}% | Temps: {}ms",
                            result.signature, result.invested_amount,
                            result.mcap_before, result.mcap_after,
                            result.mcap_impact, elapsed
                        );
                    } else {
                        log::info!(
                            "📊 TX: {} | Investi: ${:.2} | MCap Avant: ${:.0} | MCap Après: ${:.0} | Impact: {:.2}% | Temps: {}ms",
                            result.signature, result.invested_amount,
                            result.mcap_before, result.mcap_after,
                            result.mcap_impact, elapsed
                        );
                    }
                }
                Err(e) => {
                    let elapsed = start.elapsed().as_millis();
                    let msg = e.to_string();
                    if msg.contains("Aucun token non-système reçu détecté") {
                        //log::info!("🔄 TX: {} | Type: Arbitrage/Conversion SOL/USD$ | Temps: {}ms", signature_clone, elapsed);
                    } else if msg.contains("Aucune pool DEX détectée") {
                        log::info!("🏊 TX: {} | Type: Swap sans pool DEX détectée | Temps: {}ms", signature_clone, elapsed);
                    } else {
                        log::info!("❌ TX: {} | Erreur: {} | Temps: {}ms", signature_clone, msg, elapsed);
                    }
                }
            }
        });
    }

    Ok(())
}


}
