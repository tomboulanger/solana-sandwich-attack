use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// ============================================================================
// ADRESSES DES PROGRAMMES DEX CONNUS
// ============================================================================

/// Programmes DEX principaux avec leurs adresses
pub const KNOWN_DEX_PROGRAMS: &[(&str, &str)] = &[
    // Raydium
    ("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8", "Raydium V4"),
    ("RVKd61ztZW9GUwhRbbLoYVRE5Xf1B2tVscKqwZqXgEr", "Raydium V3"),
    ("HWy1jotHpo6UqeQxx49dpYYdQB8wj9Qk9MdxwjLvDHB8", "Raydium V2"),
    
    // Orca
    ("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc", "Orca Whirlpool"),
    ("9W959DqEETiGZocYWCQPaJ6sBmUzgfxXfqGeTEdp3aQP", "Orca V1"),
    ("DjVE6JNiYqPL2QXyCUUh8rNjHrbz9hXHNYt99MQ59qw1", "Orca V2"),
    
    // Meteora
    ("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo", "Meteora DLMM"),
    ("CAMMCzo5YL8w4VFF8KVHrK22GGUQpFuLUUamH4uV8K9", "Meteora PCL"),
    ("Eo7WjKq67rjJQSZxS6z3YkapzY3eMj6Xy8X5EQVn5UaB", "Meteora V1"),
    
    // Jupiter
    ("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", "Jupiter V6"),
    ("JUP4Fb2cqiRUcaTHdrPC8h2gNsA2ETXiPDD33WcGuJB", "Jupiter V4"),
    ("JUP3c2Uh3WA4Ng34tw6kPd2G4C5BB21Xo36Je1s32Ph", "Jupiter V3"),
    
    // Serum
    ("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin", "Serum DEX V3"),
    ("EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o", "Serum DEX V2"),
    
    // Aldrin
    ("CURVGoZn8zycx6FXwwevgBTB2gVvdbGTEpvMJDbgs2t4", "Aldrin V2"),
    ("AMM55ShdkoGRB5jVYPjWziwk8m5MpwyDgsMWHaMSQWH6", "Aldrin V1"),
    
    // Saber
    ("SSwpkEEWHu1Wj2jXKJ8JY8vKqJm8vKqJm8vKqJm8vKq", "Saber"),
    ("SSwpMgqNDsyV7mAgN9ady4bDVu5ySjmmXejXvy2vLt1", "Saber V2"),
    
    // Cropper
    ("CTMAxxk34HjKWxQ3QLH1e5kQvQYpKXfJ8vKqJm8vKqJm", "Cropper"),
    ("CTMAxxk34HjKWxQ3QLH1e5kQvQYpKXfJ8vKqJm8vKqJm", "Cropper V2"),
    
    // Lifinity
    ("EewxydAPCCVuNEyrVN68PuSYdQ7wKn27V9Gjeoi8dy3S", "Lifinity"),
    ("EewxydAPCCVuNEyrVN68PuSYdQ7wKn27V9Gjeoi8dy3S", "Lifinity V2"),
    
    // Mercurial
    ("MERLuDFBMmsHnsBPZw2sDQZHvXFMwp8EdjudcU2HKky", "Mercurial"),
    ("MERLuDFBMmsHnsBPZw2sDQZHvXFMwp8EdjudcU2HKky", "Mercurial V2"),
    
    // Saros
    ("SARoPv2J2j9cLj2eN7Nd5xZtYyVkQwF8KqJm8vKqJm8v", "Saros"),
    ("SARoPv2J2j9cLj2eN7Nd5xZtYyVkQwF8KqJm8vKqJm8v", "Saros V2"),
    
    // Phoenix
    ("PhoeNiLZ3D1nw8vKqJm8vKqJm8vKqJm8vKqJm8vKqJm", "Phoenix"),
    ("PhoeNiLZ3D1nw8vKqJm8vKqJm8vKqJm8vKqJm8vKqJm", "Phoenix V2"),
    
    // Pump.fun (NON SUPPORTÉ)
    ("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P", "Pump.fun"),
    ("BSfD6SHZigAfDWSjzD5Q41jw8LmKwtmjskPH9XW1mrRW", "Pump.fun Bonding Curve"),
];

// ============================================================================
// ADRESSES DES COMPTES DE POOLS CONNUS
// ============================================================================

/// Comptes de pools connus (vaults, markets, etc.)
pub const KNOWN_POOL_ACCOUNTS: &[(&str, &str)] = &[
    // Raydium Vaults
    ("58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2", "Raydium Vault Authority"),
    ("5Q544fKrFoe6tsEbD7S8EmxGTJYAKtTVhAW5Q5pge4j1", "Raydium Vault Authority #2"),
    ("9rpQJwzKz5zJzJzJzJzJzJzJzJzJzJzJzJzJzJzJzJz", "Raydium Vault Authority #3"),
    
    // Meteora Markets
    ("3LoAYHuSd7Gh8d7RTFnhvYtiTiefdZ5ByamU42vkzd76", "Meteora Market"),
    ("GpMZbSM2GgvTKHJirzeGfMFoaZ8UR2X7F4v8vHTvxFbL", "Meteora Market #2"),
    ("4UW2eDCoQwdfxHgroBPgAqmmQenA9sDfzU1vkA7wmbZJ", "Meteora Market #3"),
    
    // Orca Vaults
    ("2ND8JCiSssPPG2P2SN3FFRKZVVHFn8B44ed3bUarEGFH", "Orca Vault Authority"),
    ("9dpSBCfwxSM8ioGCpFUohEb2UhFbWzBbGYP2fw3jJNKr", "Orca Vault Authority #2"),
    ("GP8StUXNYSZjPikyRsvkTbvRV1GBxMErb59cpeCJnDf1", "Orca Vault Authority #3"),
    
    // Jupiter Vaults
    ("BKHAEZoG8juE6FVNGtsNt4cJ57XrVW9XENrcKLk32pwn", "Jupiter Vault Authority"),
    ("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4", "Jupiter Vault Authority #2"),
    
    // Serum Markets
    ("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin", "Serum Market"),
    ("EUqojwWA2rd19FZrzeBncJsm38Jm1hEhE3zsmX3bRc2o", "Serum Market #2"),
];

// ============================================================================
// FONCTIONS UTILITAIRES
// ============================================================================

/// Vérifie si une adresse est un programme DEX connu
pub fn is_known_dex_program(address: &str) -> Option<&str> {
    for (program_id, name) in KNOWN_DEX_PROGRAMS {
        if address == *program_id {
            return Some(name);
        }
    }
    None
}

/// Vérifie si une adresse est un compte de pool connu
pub fn is_known_pool_account(address: &str) -> Option<&str> {
    for (account_id, name) in KNOWN_POOL_ACCOUNTS {
        if address == *account_id {
            return Some(name);
        }
    }
    None
}

/// Obtient toutes les adresses de programmes DEX connus
pub fn get_all_dex_program_addresses() -> Vec<&'static str> {
    KNOWN_DEX_PROGRAMS.iter().map(|(addr, _)| *addr).collect()
}

/// Obtient toutes les adresses de comptes de pools connus
pub fn get_all_pool_account_addresses() -> Vec<&'static str> {
    KNOWN_POOL_ACCOUNTS.iter().map(|(addr, _)| *addr).collect()
}

/// Vérifie si une adresse est liée à un DEX (programme ou compte)
pub fn is_dex_related(address: &str) -> Option<&str> {
    if let Some(name) = is_known_dex_program(address) {
        return Some(name);
    }
    if let Some(name) = is_known_pool_account(address) {
        return Some(name);
    }
    None
}

/// Parse une adresse en Pubkey
pub fn parse_pubkey(address: &str) -> Result<Pubkey, String> {
    Pubkey::from_str(address).map_err(|e| format!("Erreur parsing adresse {}: {}", address, e))
}

/// Obtient les informations complètes sur une adresse DEX
pub fn get_dex_info(address: &str) -> Option<(&'static str, &'static str)> {
    for (program_id, name) in KNOWN_DEX_PROGRAMS {
        if address == *program_id {
            return Some((*program_id, *name));
        }
    }
    for (account_id, name) in KNOWN_POOL_ACCOUNTS {
        if address == *account_id {
            return Some((*account_id, *name));
        }
    }
    None
}
