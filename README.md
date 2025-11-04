# Sandwich Bot (Proof of Concept)

> ⚠️ **DISCLAIMER : POC uniquement – ne fonctionne pas en production !**  
> Ce projet est un proof of concept pour détecter et analyser des transactions DEX sur Solana et tenter de repérer des opportunités de sandwich.  
> Il n’est **pas fonctionnel** et nécessite de nombreuses améliorations avant toute utilisation réelle.

---

## Description du projet

Ce bot surveille les transactions sur Solana via WebSocket et tente d’identifier des opportunités de sandwich sur différents DEX comme :

- Raydium
- Orca
- Jupiter
- Serum
- Meteora

Le bot récupère les transactions, analyse leur impact sur la capitalisation de marché (MCap) des tokens impliqués et détecte les transactions potentiellement profitables pour un sandwich.  

---

## Fonctionnalités

- Surveillance des logs de transactions en temps réel via WebSocket
- Filtrage des transactions DEX importantes
- Récupération des détails des transactions via RPC
- Calcul des tokens reçus et estimation de l’impact MCap
- Détection d’opportunités de sandwich (POC)
- Analyse en parallèle des transactions

---

## Limitations et travaux à faire

Actuellement, le bot **ne fonctionne pas réellement**. Voici pourquoi :

1. **Calcul de l’impact MCap incomplet**  
   La récupération de l’impact réel de l’achat sur le MCap n’est pas encore fiable.  

2. **Nombre limité de pools et de prix pris en compte**  
   Il faudrait inclure beaucoup plus de pools pour avoir une estimation réaliste.  

3. **Calcul des prix simplifié**  
   Les prix des tokens ne sont pas calculés correctement pour les swaps complexes.  

4. **Infrastructure critique**  
   Pour fonctionner en temps réel, il faut :
   - Un RPC très performant ou un cluster premium
   - Jetstream / streaming WebSocket ultra rapide, on ne peut pas se permettre d'être un block après
   - Capacité à traiter des milliers de transactions simultanément sans latence

5. **Envoi des transactions non implémenté**  
   Le bot ne peut pas envoyer de transactions actuellement.  
   Pour un vrai bot sandwich, il faudrait :
   - Bundler la transaction de l’acheteur avec **nos deux transactions** (achat + revente)
   - Être extrêmement rapide (latence < ms)

---

## Installation et utilisation

1. Cloner le dépôt :

```bash
git clone https://github.com/tomboulanger/solana-sandwich-attack.git
cd sandwich-bot
cargo build --release
cargo run --release --bin sandwich-bot


