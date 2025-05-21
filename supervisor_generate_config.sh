#!/bin/sh

SYMBOLS_FILE="/usr/local/bin/pyth_lazer_list.json"
TMP_FILE=$(mktemp)

jq -c '.' "$SYMBOLS_FILE" > "$TMP_FILE"

SYMBOLS_COUNT=$(jq -r 'length' "$TMP_FILE")

mkdir -p /etc/supervisor/conf.d
: > /etc/supervisor/conf.d/real_time_pricing_oracle.conf

COUNTER=1
i=0
while [ "$i" -lt "$SYMBOLS_COUNT" ]; do
    GROUP=$(jq -c ".[$i:$((i+4))]" "$TMP_FILE")

    CHANNEL=$(echo "$GROUP" | jq -r '.[0].min_channel')
    PRICE_FEEDS=$(echo "$GROUP" | jq -r '[.[].name] | join(",")')

    cat >> /etc/supervisor/conf.d/real_time_pricing_oracle.conf <<EOL
[program:symbol-fetcher-$COUNTER]
command=env ORACLE_WS_URL="$ORACLE_WS_URL" \
        ORACLE_AUTH_HEADER="$ORACLE_AUTH_HEADER" \
        SOLANA_CLUSTER="$ORACLE_SOLANA_CLUSTER" \
        ORACLE_PRICE_FEEDS="$PRICE_FEEDS" \
        ORACLE_CHANNEL="$CHANNEL" \
        ephemeral-pricing-oracle
autostart=true
autorestart=true
stdout_logfile=/dev/stdout
stderr_logfile=/dev/stderr
stdout_logfile_maxbytes=0
stderr_logfile_maxbytes=0

EOL

    COUNTER=$((COUNTER+1))
    i=$((i+4))
done

rm "$TMP_FILE"
echo "Generated $((COUNTER-1)) Oracle supervisor configurations"