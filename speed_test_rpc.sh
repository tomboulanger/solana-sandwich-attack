#!/bin/bash

# Configuration
RPC_URL="https://mainnet.helius-rpc.com/?api-key=4582d53a-f359-405e-b484-65269ed35fa9"

echo "🌐 Test de latence réseau RPC Solana"
echo "📡 RPC: $RPC_URL"
echo ""

# Fonction pour mesurer le temps
measure_time() {
    local method=$1
    local params=$2
    local label=$3
    
    echo "Testing: $label"
    
    # Mesure avec curl (temps total)
    local output=$(curl -w "\n%{time_total}" -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\": \"2.0\",
            \"id\": 1,
            \"method\": \"$method\",
            \"params\": $params
        }")
    
    # Extraire le temps (dernière ligne)
    local time=$(echo "$output" | tail -n 1)
    local time_ms=$(echo "$time * 1000" | bc)
    
    echo "  Latence: ${time_ms}ms"
    echo ""
}

# Test 1: getHealth (ping simple)
echo "═══════════════════════════════════════"
echo "TEST 1: Ping simple (getHealth)"
echo "═══════════════════════════════════════"
echo "Exécution de 10 pings..."
echo ""

for i in {1..10}; do
    start=$(date +%s%N)
    curl -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"getHealth"}' > /dev/null
    end=$(date +%s%N)
    duration=$(( (end - start) / 1000000 ))
    echo "  Ping #$i: ${duration}ms"
    sleep 0.1
done

echo ""

# Test 2: getSlot (requête légère)
echo "═══════════════════════════════════════"
echo "TEST 2: getSlot (requête légère)"
echo "═══════════════════════════════════════"

for i in {1..5}; do
    curl -w "  Requête #$i: %{time_total}s\n" -o /dev/null -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"getSlot"}'
    sleep 0.1
done

echo ""

# Test 3: getVersion (meta info)
echo "═══════════════════════════════════════"
echo "TEST 3: getVersion (meta info)"
echo "═══════════════════════════════════════"

curl -w "\n  Latence: %{time_total}s\n" -X POST "$RPC_URL" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":1,"method":"getVersion"}'

echo ""

# Test 4: getTransaction (requête lourde avec processed)
echo "═══════════════════════════════════════"
echo "TEST 4: getTransaction - processed"
echo "═══════════════════════════════════════"

TX_SIG="3ACKArLMe9zeebAFbM7sHBRTydbK88NyWHHbRPbKBYGtMdsD95r7kDXvUXANUtpYP9YJgLHi9Scg2yCRrxzrF8c2"

for i in {1..3}; do
    curl -w "  Requête #$i (processed): %{time_total}s\n" -o /dev/null -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\":\"2.0\",
            \"id\":1,
            \"method\":\"getTransaction\",
            \"params\":[
                \"$TX_SIG\",
                {\"encoding\":\"jsonParsed\",\"commitment\":\"processed\",\"maxSupportedTransactionVersion\":0}
            ]
        }"
    sleep 0.1
done

echo ""

# Test 5: getTransaction (requête lourde avec confirmed)
echo "═══════════════════════════════════════"
echo "TEST 5: getTransaction - confirmed"
echo "═══════════════════════════════════════"

for i in {1..3}; do
    curl -w "  Requête #$i (confirmed): %{time_total}s\n" -o /dev/null -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\":\"2.0\",
            \"id\":1,
            \"method\":\"getTransaction\",
            \"params\":[
                \"$TX_SIG\",
                {\"encoding\":\"jsonParsed\",\"commitment\":\"confirmed\",\"maxSupportedTransactionVersion\":0}
            ]
        }"
    sleep 0.1
done

echo ""

# Test 6: getTokenSupply (always confirmed)
echo "═══════════════════════════════════════"
echo "TEST 6: getTokenSupply (confirmed only)"
echo "═══════════════════════════════════════"

# Token mint example (remplacer par un token réel)
TOKEN_MINT="EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v" # USDC

for i in {1..3}; do
    curl -w "  Requête #$i: %{time_total}s\n" -o /dev/null -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\":\"2.0\",
            \"id\":1,
            \"method\":\"getTokenSupply\",
            \"params\":[\"$TOKEN_MINT\"]
        }"
    sleep 0.1
done

echo ""
echo "✅ Tests terminés"
echo ""
echo "💡 Interprétation des résultats:"
echo "  < 50ms   : Excellent (RPC premium proche)"
echo "  50-100ms : Très bon (RPC premium standard)"
echo "  100-200ms: Bon (RPC payant basique)"
echo "  200-500ms: Moyen (RPC gratuit optimisé)"
echo "  > 500ms  : Lent (RPC public ou problème réseau)"