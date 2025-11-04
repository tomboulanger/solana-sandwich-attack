use sandwich_bot::*;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Charger les variables d'environnement depuis .env
    dotenv::dotenv().ok();
    
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    let config = BotConfig::new();
    
    let mut bot = SandwichBot::new(config).await?;
    bot.start().await?;

    Ok(())
}
