#!/bin/bash

set -e

API_ENDPOINT="https://stage.api.indexer.intmax.io/v1/proxy/meta"
INDEXER_API_ENDPOINT="https://stage.api.indexer.intmax.io"
BUILDER_SCRIPT_URL="https://raw.githubusercontent.com/InternetMaximalism/intmax2/refs/heads/main/scripts/block-builder-testnet.sh"

EXPECTED_NETWORK_ID="534351"
EXPECTED_NETWORK_NAME="Scroll Sepolia Testnet"
ENVIRONMENT="testnet"

ALLOWED_DOMAINS="intmax.io,raw.githubusercontent.com"

# .env
REGISTRATION_FEE=0:2500000000000
NON_REGISTRATION_FEE=0:2000000000000
STORE_VAULT_SERVER_BASE_URL=https://stage.api.node.intmax.io/store-vault-server
VALIDITY_PROVER_BASE_URL=https://stage.api.node.intmax.io/validity-prover
ROLLUP_CONTRACT_ADDRESS=0xcEC03800074d0ac0854bF1f34153cc4c8bAEeB1E
BLOCK_BUILDER_REGISTRY_CONTRACT_ADDRESS=0x93a41F47ed161AB2bc58801F07055f2f05dfc74E

# api
INTMAX2_VERSION=""
PROXY_DOMAIN=""
FRP_TOKEN=""

set_env_from_environment() {
    case "$ENVIRONMENT" in
        "devnet")
            ENV="dev"
            ;;
        "testnet")
            ENV="staging"
            ;;
        "mainnet")
            ENV="prod"
            ;;
        *)
            echo "❌ Unknown ENVIRONMENT: $ENVIRONMENT"
            echo "💡 Supported values: devnet, testnet, mainnet"
            return 1
            ;;
    esac

    return 0
}

if set_env_from_environment; then
    :
else
    echo "❌ Failed to set ENV"
    exit 1
fi

show_current_environment() {
    if set_env_from_environment; then
        local env_color=""
        local env_label=""
        case "$ENVIRONMENT" in
            "devnet")
                env_color="\033[1;36m"
                env_label="🔧 DEVELOPMENT"
                ;;
            "testnet")
                env_color="\033[1;33m"
                env_label="🧪 TESTNET"
                ;;
            "mainnet")
                env_color="\033[1;32m"
                env_label="🚀 MAINNET"
                ;;
        esac
        local reset_color="\033[0m"

        echo ""
        echo "═══════════════════════════════════════════════════════════════"
        echo "🌍 ENVIRONMENT CONFIGURATION"
        echo -e "   ENVIRONMENT: ${env_color}${ENVIRONMENT}${reset_color}"
        echo "═══════════════════════════════════════════════════════════════"
        echo ""
    else
        echo "   ❌ INVALID ENVIRONMENT: $ENVIRONMENT"
        echo "   💡 Supported values: devnet, testnet, mainnet"
        echo ""
        return 1
    fi
}

validate_api_endpoint() {
    local endpoint="$1"

    if ! echo "$endpoint" | grep -q "^https://"; then
        echo "❌ API endpoint must use HTTPS"
        return 1
    fi

    local domain=$(echo "$endpoint" | sed 's|https://||' | cut -d'/' -f1)

    IFS=',' read -ra allowed_domains_array <<< "$ALLOWED_DOMAINS"

    local domain_allowed=false
    for allowed_domain in "${allowed_domains_array[@]}"; do
        allowed_domain=$(echo "$allowed_domain" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

        if [ "$domain" = "$allowed_domain" ] || echo "$domain" | grep -q "\\.${allowed_domain}$"; then
            domain_allowed=true
            break
        fi
    done

    if [ "$domain_allowed" = true ]; then
        return 0
    else
        echo "❌ Unauthorized API domain: $domain"
        echo "   Allowed domains: $ALLOWED_DOMAINS"
        return 1
    fi
}

validate_api_response() {
    local response="$1"

    if [ -z "$response" ]; then
        echo "❌ Empty API response"
        return 1
    fi

    if ! echo "$response" | jq empty 2>/dev/null; then
        echo "❌ Invalid JSON response from API"
        return 1
    fi

    local domain=$(echo "$response" | jq -r '.domain // empty')
    local token=$(echo "$response" | jq -r '.token // empty')
    local version=$(echo "$response" | jq -r '.version // empty')

    if [ -z "$domain" ] || [ -z "$token" ] || [ -z "$version" ]; then
        echo "❌ Missing required fields in API response"
        return 1
    fi

    if ! echo "$domain" | grep -E '^[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$' >/dev/null; then
        echo "❌ Invalid domain format: $domain"
        return 1
    fi

    if ! echo "$version" | grep -E '^v?[0-9]+\.[0-9]+\.[0-9]+' >/dev/null; then
        echo "❌ Invalid version format: $version"
        return 1
    fi

    if echo "$token" | grep -E '[[:space:]<>"\|&;`$()]' >/dev/null; then
        echo "❌ Token contains invalid characters"
        return 1
    fi

    return 0
}

parse_api_response() {
    local response="$1"

    if command -v jq >/dev/null 2>&1; then
        PROXY_DOMAIN=$(echo "$response" | jq -r '.domain // empty')
        FRP_TOKEN=$(echo "$response" | jq -r '.token // empty')
        local api_version=$(echo "$response" | jq -r '.version // empty')

        if [ -n "$api_version" ]; then
            INTMAX2_VERSION=$(echo "$api_version" | sed 's/^v//')
        fi
    else
        echo "⚠️  jq not found, using basic parsing"
        PROXY_DOMAIN=$(echo "$response" | grep -o '"domain":"[^"]*"' | cut -d':' -f2 | tr -d '"')
        FRP_TOKEN=$(echo "$response" | grep -o '"token":"[^"]*"' | cut -d':' -f2 | tr -d '"')
        local api_version=$(echo "$response" | grep -o '"version":"[^"]*"' | cut -d':' -f2 | tr -d '"')

        if [ -n "$api_version" ]; then
            INTMAX2_VERSION=$(echo "$api_version" | sed 's/^v//')
        fi
    fi

    if [ -z "$PROXY_DOMAIN" ] || [ -z "$FRP_TOKEN" ] || [ -z "$INTMAX2_VERSION" ]; then
        echo "❌ Failed to parse required values from API response"
        return 1
    fi

    return 0
}

sanitize_config_values() {
    if [ -n "$PROXY_DOMAIN" ]; then
        local sanitized_domain=$(echo "$PROXY_DOMAIN" | sed 's/[^a-zA-Z0-9.-]//g')

        if echo "$sanitized_domain" | grep -qE '^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$'; then
            IFS=',' read -ra allowed_domains_array <<< "$ALLOWED_DOMAINS"

            local domain_allowed=false
            for allowed_domain in "${allowed_domains_array[@]}"; do
                allowed_domain=$(echo "$allowed_domain" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

                if [ "$sanitized_domain" = "$allowed_domain" ] || echo "$sanitized_domain" | grep -qE "\\.${allowed_domain}$"; then
                    domain_allowed=true
                    break
                fi
            done

            if [ "$domain_allowed" = true ]; then
                PROXY_DOMAIN="$sanitized_domain"
            else
                echo "❌ PROXY_DOMAIN contains unauthorized domain: $sanitized_domain"
                echo "   Allowed domains: $ALLOWED_DOMAINS"
                return 1
            fi
        else
            echo "❌ PROXY_DOMAIN has invalid format after sanitization: $sanitized_domain"
            return 1
        fi
    else
        echo "❌ PROXY_DOMAIN is empty"
        return 1
    fi

    if [ -n "$FRP_TOKEN" ]; then
        local sanitized_token=$(echo "$FRP_TOKEN" | sed 's/[^a-zA-Z0-9_-]//g')

        local token_length=$(echo -n "$sanitized_token" | wc -c)
        if [ "$token_length" -ge 32 ] && [ "$token_length" -le 128 ]; then
            if echo "$sanitized_token" | grep -qE '^[a-zA-Z0-9][a-zA-Z0-9_-]*[a-zA-Z0-9]$'; then
                FRP_TOKEN="$sanitized_token"
            else
                echo "❌ FRP_TOKEN has invalid pattern after sanitization"
                return 1
            fi
        else
            echo "❌ FRP_TOKEN length invalid after sanitization: $token_length (expected: 32-128)"
            return 1
        fi
    else
        echo "❌ FRP_TOKEN is empty"
        return 1
    fi

    if [ -n "$INTMAX2_VERSION" ]; then
        local sanitized_version=$(echo "$INTMAX2_VERSION" | sed 's/[^a-zA-Z0-9.-]//g')

        if echo "$sanitized_version" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+(-[a-zA-Z0-9.-]+)?$'; then
            local major=$(echo "$sanitized_version" | cut -d'.' -f1)
            local minor=$(echo "$sanitized_version" | cut -d'.' -f2)
            local patch=$(echo "$sanitized_version" | cut -d'.' -f3 | cut -d'-' -f1)

            if [ "$major" -ge 0 ] && [ "$major" -le 999 ] && \
               [ "$minor" -ge 0 ] && [ "$minor" -le 999 ] && \
               [ "$patch" -ge 0 ] && [ "$patch" -le 999 ]; then
                if [ "$major" -eq 0 ] && [ "$minor" -eq 0 ] && [ "$patch" -eq 0 ]; then
                    echo "❌ INTMAX2_VERSION cannot be 0.0.0: $sanitized_version"
                    return 1
                fi
                INTMAX2_VERSION="$sanitized_version"
            else
                echo "❌ INTMAX2_VERSION out of valid range: $sanitized_version"
                echo "   Expected: 0.0.1 to 999.999.999 (excluding 0.0.0)"
                return 1
            fi
        else
            echo "❌ INTMAX2_VERSION has invalid semantic version format: $sanitized_version"
            return 1
        fi
    else
        echo "❌ INTMAX2_VERSION is empty"
        return 1
    fi

    if [ -z "$PROXY_DOMAIN" ] || [ -z "$FRP_TOKEN" ] || [ -z "$INTMAX2_VERSION" ]; then
        echo "❌ One or more configuration values became empty after sanitization"
        echo "   PROXY_DOMAIN: ${PROXY_DOMAIN:-'<empty>'}"
        echo "   FRP_TOKEN: ${FRP_TOKEN:+<set>}${FRP_TOKEN:-'<empty>'}"
        echo "   INTMAX2_VERSION: ${INTMAX2_VERSION:-'<empty>'}"
        return 1
    fi

    return 0
}

fetch_api_config() {
    if ! validate_api_endpoint "$API_ENDPOINT"; then
        return 1
    fi

    if ! command -v curl >/dev/null 2>&1; then
        echo "❌ curl not found. Please install curl to fetch API configuration."
        return 1
    fi

    local api_response
    local curl_options="-s --max-time 10 --fail"

    if api_response=$(curl $curl_options "$API_ENDPOINT" 2>/dev/null); then
        if validate_api_response "$api_response"; then
            if parse_api_response "$api_response"; then
                if sanitize_config_values; then
                    return 0
                fi
            fi
        fi
    fi

    echo "⚠️  API fetch failed or validation failed"
    return 1
}

load_config() {
    if ! fetch_api_config; then
        echo "❌ API fetch failed, cannot proceed without valid configuration"
        echo "Please check your internet connection and API endpoint:"
        echo "   $API_ENDPOINT"
        exit 1
    fi
}

detect_architecture() {
    local arch=$(uname -m)
    case $arch in
        aarch64|arm64)
            echo "ghcr.io/internetmaximalism/intmax2:${INTMAX2_VERSION}-arm64"
            ;;
        x86_64|amd64)
            echo "ghcr.io/internetmaximalism/intmax2:${INTMAX2_VERSION}"
            ;;
        *)
            echo "⚠️  Unknown architecture: $arch. Using default x86_64 image."
            echo "ghcr.io/internetmaximalism/intmax2:${INTMAX2_VERSION}"
            ;;
    esac
}

generate_uuid() {
    if ! command -v uuidgen >/dev/null 2>&1; then
        echo "❌ uuidgen is required but not found" >&2
        echo "" >&2
        echo "Please install uuidgen:" >&2
        echo "" >&2
        return 1
    fi

    uuidgen | tr '[:upper:]' '[:lower:]'
}

check_required_tools() {
    local missing_tools=()

    if ! command -v uuidgen >/dev/null 2>&1; then
        missing_tools+=("uuidgen")
    fi

    if ! command -v curl >/dev/null 2>&1; then
        missing_tools+=("curl")
    fi

    if ! command -v docker >/dev/null 2>&1; then
        missing_tools+=("docker")
    fi

    local recommended_tools=()
    if ! command -v jq >/dev/null 2>&1; then
        recommended_tools+=("jq")
    fi

    if [ ${#missing_tools[@]} -gt 0 ]; then
        echo "❌ Missing required tools: ${missing_tools[*]}"
        echo ""
        echo "Installation commands:"

        for tool in "${missing_tools[@]}"; do
            echo "Missing tool: $tool"
            echo "Please install $tool using your system's package manager:"
            echo ""
        done

        return 1
    fi

    if [ ${#recommended_tools[@]} -gt 0 ]; then
        echo "⚠️  Recommended tools not found: ${recommended_tools[*]}"
        echo "   These tools will improve the script's functionality"
        echo ""
    fi

    echo "✅ All required tools are available"
    return 0
}

check_docker_swarm() {
    if ! docker info 2>/dev/null | grep -q "Swarm: active"; then
        echo "⚠️  Docker Swarm is not active"
        echo "💡 To enable Docker Swarm: docker swarm init"
        echo "🔄 After running 'docker swarm init', please re-execute the command"
        return 1
    fi
    return 0
}

check_docker_secret() {
    if ! check_docker_swarm; then
        return 1
    fi

    local secret_name="block_builder_private_key_${ENVIRONMENT}"

    if docker secret ls 2>/dev/null | grep -q "$secret_name"; then
        echo "✅ Docker secret '$secret_name' exists"
        return 0
    else
        echo "❌ Docker secret '$secret_name' not found"
        echo "💡 Run: $0 setup-env"
        return 1
    fi
}

check_private_key_config() {
    local has_docker_secret=false
    local secret_name="block_builder_private_key_${ENVIRONMENT}"

    if check_docker_swarm >/dev/null 2>&1; then
        if docker secret ls 2>/dev/null | grep -q "$secret_name"; then
            has_docker_secret=true
            echo "✅ Docker secret '$secret_name' exists"
        fi
    fi

    if [ "$has_docker_secret" = false ]; then
        echo "❌ No private key configuration found"
        echo "💡 Please set up environment using:"
        echo "   $0 setup-env"
        return 1
    fi

    return 0
}

check_rpc_connectivity() {
    local l2_rpc_url="$1"
    local verbose="${2:-false}"

    if [ -z "$l2_rpc_url" ]; then
        echo "❌ RPC URL is required"
        return 1
    fi

    if ! command -v curl >/dev/null 2>&1; then
        echo "⚠️  curl not available, skipping connectivity test"
        return 0
    fi

    if [ "$verbose" = true ]; then
        echo "🔗 Testing L2 RPC connectivity..."
        echo "   Testing: $l2_rpc_url"
    fi

    local rpc_test_payload='{"jsonrpc":"2.0","method":"net_version","params":[],"id":1}'
    local curl_start_time=$(date +%s)
    local response_body
    local http_code

    if [ "$verbose" = true ]; then
        echo "   Method: net_version"
    fi

    if response_body=$(curl -s --connect-timeout 10 --max-time 15 \
        -H "Content-Type: application/json" \
        -d "$rpc_test_payload" \
        -w "%{http_code}" \
        "$l2_rpc_url" 2>/dev/null); then

        http_code="${response_body: -3}"
        response_body="${response_body%???}"

        local curl_end_time=$(date +%s)
        local response_time=$((curl_end_time - curl_start_time))

        if [ "$http_code" -eq 200 ]; then
            if [ "$verbose" = true ]; then
                echo "   ✅ RPC connectivity test passed (HTTP $http_code, ${response_time}s)"
            fi

            if command -v jq >/dev/null 2>&1 && echo "$response_body" | jq empty 2>/dev/null; then
                local result=$(echo "$response_body" | jq -r '.result // empty')
                local error=$(echo "$response_body" | jq -r '.error.message // empty')

                if [ -n "$result" ]; then
                    if [ "$verbose" = true ]; then
                        echo "   📊 Network ID: $result"

                        if [ "$result" = "$EXPECTED_NETWORK_ID" ]; then
                            echo "   🌐 Network: $EXPECTED_NETWORK_NAME ✅"
                        else
                            echo "   🌐 Network: Chain ID $result"
                            echo "   ⚠️  Note: Expected $EXPECTED_NETWORK_NAME ($EXPECTED_NETWORK_ID) for this setup"
                        fi
                    fi
                elif [ -n "$error" ]; then
                    if [ "$verbose" = true ]; then
                        echo "   ⚠️  RPC returned error: $error"
                    fi
                    return 1
                else
                    if [ "$verbose" = true ]; then
                        echo "   ⚠️  Unexpected RPC response format"
                    fi
                fi
            else
                if [ "$verbose" = true ]; then
                    echo "   📄 Response received (jq not available for detailed parsing)"
                    echo "   Response preview: $(echo "$response_body" | cut -c1-100)..."
                fi
            fi

            if [ "$verbose" = true ]; then
                echo ""
                echo "🔗 Testing latest block retrieval..."
            fi

            local block_test_payload='{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":2}'

            if response_body=$(curl -s --connect-timeout 10 --max-time 15 \
                -H "Content-Type: application/json" \
                -d "$block_test_payload" \
                -w "%{http_code}" \
                "$l2_rpc_url" 2>/dev/null); then

                http_code="${response_body: -3}"
                response_body="${response_body%???}"

                if [ "$http_code" -eq 200 ]; then
                    if [ "$verbose" = true ]; then
                        echo "   ✅ Block number retrieval passed (HTTP $http_code)"
                    fi

                    if command -v jq >/dev/null 2>&1 && echo "$response_body" | jq empty 2>/dev/null; then
                        local block_hex=$(echo "$response_body" | jq -r '.result // empty')
                        if [ -n "$block_hex" ] && [ "$block_hex" != "null" ]; then
                            if command -v printf >/dev/null 2>&1; then
                                local block_num=$(printf "%d" "$block_hex" 2>/dev/null || echo "$block_hex")
                                if [ "$verbose" = true ]; then
                                    echo "   📊 Latest block: $block_num ($block_hex)"
                                fi
                            else
                                if [ "$verbose" = true ]; then
                                    echo "   📊 Latest block: $block_hex"
                                fi
                            fi
                        else
                            if [ "$verbose" = true ]; then
                                echo "   ⚠️  Could not retrieve block number"
                            fi
                        fi
                    fi
                else
                    if [ "$verbose" = true ]; then
                        echo "   ⚠️  Block number test failed (HTTP $http_code)"
                    fi
                fi
            else
                if [ "$verbose" = true ]; then
                    echo "   ⚠️  Block number test connection failed"
                fi
            fi

            return 0

        elif [ "$http_code" -eq 405 ]; then
            if [ "$verbose" = true ]; then
                echo "   ❌ RPC endpoint doesn't support POST method (HTTP $http_code)"
                echo "   💡 Check if the URL is correct and supports JSON-RPC"
            fi
            return 1
        elif [ "$http_code" -eq 404 ]; then
            if [ "$verbose" = true ]; then
                echo "   ❌ RPC endpoint not found (HTTP $http_code)"
                echo "   💡 Check if the URL path is correct"
            fi
            return 1
        elif [ "$http_code" -ge 500 ]; then
            if [ "$verbose" = true ]; then
                echo "   ❌ RPC server error (HTTP $http_code)"
                echo "   💡 The RPC server might be temporarily unavailable"
            fi
            return 1
        else
            if [ "$verbose" = true ]; then
                echo "   ❌ RPC connectivity test failed (HTTP $http_code)"
                if [ -n "$response_body" ]; then
                    echo "   Response: $(echo "$response_body" | cut -c1-200)..."
                fi
            fi
            return 1
        fi
    else
        if [ "$verbose" = true ]; then
            echo "   ❌ Cannot reach L2 RPC endpoint"
            echo "   💡 Check your internet connection and RPC URL"
            echo "   💡 If using a local node, ensure it's running and accessible"
        fi
        return 1
    fi
}

confirm_action() {
    local message="${1:-Are you sure?}"
    local default="${2:-N}"

    echo "❓ $message (y/n)"

    read -p "Enter your choice: " confirm

    case "$confirm" in
        [yY]|[yY][eE][sS])
            echo "✅ Proceeding..."
            return 0
            ;;
        [nN]|[nN][oO])
            echo "❌ Operation cancelled"
            return 1
            ;;
        "")
            if [[ "$default" == "Y" || "$default" == "y" ]]; then
                echo "✅ Proceeding (default: Yes)..."
                return 0
            else
                echo "❌ Operation cancelled (default: No)"
                return 1
            fi
            ;;
        *)
            echo "❌ Invalid input. Operation cancelled"
            return 1
            ;;
    esac
}

setup() {
    show_current_environment

    echo "🔍 Checking required tools..."
    if ! check_required_tools; then
        echo "❌ Setup cannot continue without required tools"
        echo "Please install the missing tools and try again"
        exit 1
    fi
    echo ""

    if [ -f "frpc.toml" ] || [ -f "docker-compose.yml" ] || [ -f ".env.${ENVIRONMENT}" ] || [ -f "nginx.conf" ]; then
        echo "⚠️  Setup files already exist. The following files were found:"
        [ -f "frpc.toml" ] && echo "   - frpc.toml"
        [ -f "docker-compose.yml" ] && echo "   - docker-compose.yml"
        [ -f ".env.${ENVIRONMENT}" ] && echo "   - .env.${ENVIRONMENT}"
        [ -f "nginx.conf" ] && echo "   - nginx.conf"
        echo ""
        echo "🧹 Please run cleanup first before setting up again:"
        echo "   $0 clean"
        echo ""
        echo "💡 Or if you want to start fresh automatically:"
        echo "   $0 clean && $0 setup"
        return 1
    fi
    load_config

    uuid=$(generate_uuid)
    if [ $? -ne 0 ]; then
        echo "❌ Failed to generate UUID"
        exit 1
    fi

    docker_image=$(detect_architecture)

    cat > frpc.toml << EOF
serverAddr = "$PROXY_DOMAIN"
serverPort = 7000
auth.token = "$FRP_TOKEN"

[[proxies]]
name = "$uuid-block-builder"
type = "http"
localIP = "nginx-proxy-$ENVIRONMENT"
localPort = 3000
customDomains = ["$PROXY_DOMAIN"]
locations = ["/$uuid"]
EOF

    cat > nginx.conf << EOF
events {
    worker_connections 1024;
}
http {
    upstream block_builder_${ENVIRONMENT} {
        server block-builder-${ENVIRONMENT}:8080;
    }
    server {
        listen 3000;
        location ~ "^/([^/]+)(/.*)$" {
            proxy_pass http://block_builder_${ENVIRONMENT}\$2;
            proxy_set_header Host \$host;
            proxy_set_header X-Real-IP \$remote_addr;
            proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
            proxy_set_header X-Namespace \$1;
        }
        location / {
            return 404;
        }
    }
}
EOF

    cat > docker-compose.yml << 'EOF'
services:
  nginx-proxy-ENVIRONMENT:
    image: nginx:alpine
    ports:
      - "3000:3000"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    networks:
      - builder-network-ENVIRONMENT
    tmpfs:
      - /var/cache/nginx
      - /var/run
      - /tmp
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"

  block-builder-ENVIRONMENT:
    image: DOCKER_IMAGE_PLACEHOLDER
    command:
      [
        "export BLOCK_BUILDER_PRIVATE_KEY=$$(cat /run/secrets/block_builder_private_key_ENVIRONMENT | tr -d '\n') && exec /app/block-builder",
      ]
    env_file:
      - .env.ENVIRONMENT
    environment:
      - PORT=8080
      - BLOCK_BUILDER_URL=https://PROXY_DOMAIN_PLACEHOLDER/UUID_PLACEHOLDER
    secrets:
      - block_builder_private_key_ENVIRONMENT
    networks:
      - builder-network-ENVIRONMENT
    healthcheck:
      disable: true
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"

  frp-client-ENVIRONMENT:
    image: snowdreamtech/frpc:latest
    volumes:
      - ./frpc.toml:/etc/frp/frpc.toml:ro
    networks:
      - builder-network-ENVIRONMENT
    logging:
      driver: "json-file"
      options:
        max-size: "10m"
        max-file: "3"

networks:
  builder-network-ENVIRONMENT:
    driver: overlay
    attachable: true

secrets:
  block_builder_private_key_ENVIRONMENT:
    external: true
EOF

sed -i.tmp "s|DOCKER_IMAGE_PLACEHOLDER|$docker_image|g" docker-compose.yml && rm -f docker-compose.yml.tmp
sed -i.tmp "s|PROXY_DOMAIN_PLACEHOLDER|$PROXY_DOMAIN|g" docker-compose.yml && rm -f docker-compose.yml.tmp
sed -i.tmp "s|UUID_PLACEHOLDER|$uuid|g" docker-compose.yml && rm -f docker-compose.yml.tmp
sed -i.tmp "s|ENVIRONMENT|$ENVIRONMENT|g" docker-compose.yml && rm -f docker-compose.yml.tmp

    cat > ".env.${ENVIRONMENT}" << EOF
#######
# Contents of .env.${ENVIRONMENT} for ${ENVIRONMENT}
#######

# app settings
PORT=8080

# builder settings
ETH_ALLOWANCE_FOR_BLOCK=0.001
TX_TIMEOUT=80
ACCEPTING_TX_INTERVAL=30
PROPOSING_BLOCK_INTERVAL=30
INITIAL_HEART_BEAT_DELAY=180
HEART_BEAT_INTERVAL=85800
GAS_LIMIT_FOR_BLOCK_POST=400000
CLUSTER_ID=1

# fee settings
REGISTRATION_FEE=${REGISTRATION_FEE}
NON_REGISTRATION_FEE=${NON_REGISTRATION_FEE}

# external settings
ENV=${ENV}
STORE_VAULT_SERVER_BASE_URL=${STORE_VAULT_SERVER_BASE_URL}
USE_S3=true
VALIDITY_PROVER_BASE_URL=${VALIDITY_PROVER_BASE_URL}
L2_RPC_URL=<your-rpc-url>
ROLLUP_CONTRACT_ADDRESS=${ROLLUP_CONTRACT_ADDRESS}
BLOCK_BUILDER_REGISTRY_CONTRACT_ADDRESS=${BLOCK_BUILDER_REGISTRY_CONTRACT_ADDRESS}
EOF

    echo "✅ Configuration files created with UUID: $uuid"
    echo "🏗️  Architecture: $(uname -m)"
    echo "🐳 Docker image: $docker_image"
    echo "🌐 Proxy domain: $PROXY_DOMAIN"
    echo "🔗 Block builder URL: https://$PROXY_DOMAIN/$uuid"
    echo "📄 Files created:"
    echo "   - frpc.toml"
    echo "   - nginx.conf"
    echo "   - docker-compose.yml"
    echo "   - .env.${ENVIRONMENT}"
    echo ""
    echo "🔧 Next steps:"
    echo "   1. Set up env: $0 setup-env"
    echo "   2. Run: $0 check"
    echo "   3. Run: $0 run"
}

setup_env() {
    echo "🌍 Setting up environment configuration..."

    local env_file=".env.${ENVIRONMENT}"

    if [ ! -f "$env_file" ]; then
        echo "❌ $env_file file not found"
        echo "💡 Run: $0 setup first to create the initial $env_file file"
        return 1
    fi

    echo "🐳 Checking Docker status..."
    if ! docker info >/dev/null 2>&1; then
        echo "❌ Docker is not running or not accessible"
        echo "💡 Please start Docker and try again"
        return 1
    fi

    if ! docker info 2>/dev/null | grep -q "Swarm: active"; then
        echo "❌ Docker Swarm is not active"
        echo "💡 Run: docker swarm init"
        echo "🔄 After running 'docker swarm init', please re-execute the command"
        return 1
    fi

    echo ""
    echo "🔧 This command will configure:"
    echo "   1. L2_RPC_URL in .env file"
    echo "   2. Private key as Docker secret"
    echo ""

    echo "📝 L2 RPC URL configuration:"
    echo "   This should be a valid HTTP/HTTPS URL to your Scroll (Sepolia) RPC endpoint"
    echo "   Examples:"
    echo ""
    echo "Mainnet RPC URLs:"
    echo "  • https://scroll-sepolia.infura.io/v3/YOUR_PROJECT_ID"
    echo "  • https://scroll-sepolia.g.alchemy.com/v2/YOUR_API_KEY"
    echo ""
    echo "Testnet RPC URLs:"
    echo "  • https://scroll-sepolia.infura.io/v3/YOUR_PROJECT_ID"
    echo "  • https://scroll-sepolia.g.alchemy.com/v2/YOUR_API_KEY"
    echo ""

    update_rpc=true
    current_rpc_url=$(grep "^L2_RPC_URL=" "$env_file" 2>/dev/null | cut -d'=' -f2-)
    if [ -n "$current_rpc_url" ] && [ "$current_rpc_url" != "<your-rpc-url>" ]; then
        echo "🔄 Current L2_RPC_URL: $current_rpc_url"
        echo "🔄 Do you want to update the existing L2_RPC_URL? (y/n)"
        echo -n "> "
        read -r update_rpc_choice

        if [ "$update_rpc_choice" != "y" ] && [ "$update_rpc_choice" != "Y" ]; then
            echo "ℹ️  Keeping existing L2_RPC_URL configuration"
            l2_rpc_url="$current_rpc_url"
            update_rpc=false
        fi
    fi

    if [ "$update_rpc" = true ]; then
        echo ""
        echo "🌐 Please enter your L2 RPC URL:"
        echo -n "> "
        read -r l2_rpc_url
        echo ""

        if [ -z "$l2_rpc_url" ]; then
            echo "❌ L2 RPC URL cannot be empty"
            return 1
        fi

        if ! echo "$l2_rpc_url" | grep -qE '^https?://[a-zA-Z0-9.-]+'; then
            echo "❌ Invalid URL format"
            echo "   L2 RPC URL must start with http:// or https://"
            echo "   Your input: $l2_rpc_url"
            return 1
        fi

        l2_rpc_url=$(echo "$l2_rpc_url" | sed 's|/$||')
    fi

    echo ""
    echo "🔐 Private key configuration:"

    local secret_name="block_builder_private_key_${ENVIRONMENT}"

    update_private_key=true
    if docker secret ls | grep -q "$secret_name"; then
        echo "⚠️  Docker secret '$secret_name' already exists"
        echo ""
        echo "🔄 Do you want to overwrite the existing private key? (y/n)"
        echo -n "> "
        read -r overwrite_choice

        if [ "$overwrite_choice" != "y" ] && [ "$overwrite_choice" != "Y" ]; then
            echo "ℹ️  Keeping existing private key configuration"
            update_private_key=false
        else
            echo "🗑️  Removing existing secret..."
            docker secret rm "$secret_name" || {
                echo "❌ Failed to remove existing secret"
                return 1
            }
            echo "✅ Existing secret removed"
        fi
    fi

    if [ "$update_private_key" = true ]; then
        echo ""
        echo "📝 Private key format requirements:"
        echo "   - Can be with or without '0x' prefix"
        echo "   - 64 characters (raw hex) or 66 characters (with 0x)"
        echo "   - Examples:"
        echo "     • 0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        echo "     •   1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef"
        echo ""
        echo "🔐 Please enter your private key:"
        echo -n "> "
        read -s private_key
        echo ""

        if [ -z "$private_key" ]; then
            echo "❌ Private key cannot be empty"
            return 1
        fi

        private_key=$(echo "$private_key" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')

        has_0x_prefix=false
        if echo "$private_key" | grep -q "^0x"; then
            has_0x_prefix=true
            hex_part=$(echo "$private_key" | cut -c3-)
            expected_length=66
        else
            hex_part="$private_key"
            expected_length=64
        fi

        key_length=$(echo -n "$private_key" | wc -c)
        hex_length=$(echo -n "$hex_part" | wc -c)

        if [ "$key_length" -ne "$expected_length" ]; then
            echo "❌ Invalid private key length"
            echo "   Your input length: $key_length characters"
            if [ "$has_0x_prefix" = true ]; then
                echo "   Expected: 66 characters (0x + 64 hex characters)"
            else
                echo "   Expected: 64 characters (raw hex) or 66 characters (with 0x prefix)"
            fi
            return 1
        fi

        if ! echo "$hex_part" | grep -q "^[0-9a-fA-F]\{64\}$"; then
            echo "❌ Private key contains invalid characters"
            echo "   Only hexadecimal characters (0-9, a-f, A-F) are allowed"
            if [ "$has_0x_prefix" = true ]; then
                echo "   Invalid part: $(echo "$hex_part" | cut -c1-10)..."
            else
                echo "   Invalid part: $(echo "$private_key" | cut -c1-10)..."
            fi
            return 1
        fi

        if [ "$has_0x_prefix" = false ]; then
            echo "💡 Adding '0x' prefix to private key"
            private_key="0x$private_key"
            key_length=66
        fi
    fi

    echo ""
    echo "💾 Applying configurations..."

    if [ "$update_rpc" = true ]; then
        if grep -q "^L2_RPC_URL=" "$env_file"; then
            sed -i.tmp "s|^L2_RPC_URL=.*|L2_RPC_URL=$l2_rpc_url|" "$env_file" && rm -f "${env_file}.tmp"
            echo "✅ Updated L2_RPC_URL in $env_file file"
        else
            echo "L2_RPC_URL=$l2_rpc_url" >> "$env_file"
            echo "✅ Added L2_RPC_URL to $env_file file"
        fi
    else
        echo "ℹ️  L2_RPC_URL unchanged"
    fi

    if [ "$update_private_key" = true ]; then
       echo "$private_key" | docker secret create "$secret_name" - 2>/dev/null || {
            echo "❌ Failed to create Docker secret"
            echo "💡 Make sure Docker Swarm is properly initialized"
            return 1
        }
        echo "✅ Private key stored as Docker secret: $secret_name"
    else
        echo "ℹ️  Private key unchanged"
    fi

    echo ""
    echo "📊 Configuration Summary:"
    echo "   L2_RPC_URL: $l2_rpc_url"

    if echo "$l2_rpc_url" | grep -q "localhost\|127.0.0.1"; then
        echo "   RPC Type: Local endpoint ⚠️"
        echo "   Note: Make sure your local node is accessible from Docker containers"
    elif echo "$l2_rpc_url" | grep -q "^https://"; then
        echo "   RPC Type: HTTPS endpoint ✅"
    elif echo "$l2_rpc_url" | grep -q "^http://"; then
        echo "   RPC Type: HTTP endpoint ⚠️"
        echo "   Note: Consider using HTTPS for production"
    fi

    if [ "$update_private_key" = true ]; then
        echo "   Private key length: $key_length characters ✅"
        if [ "$has_0x_prefix" = false ]; then
            echo "   Private key format: 0x prefix added ✅"
        else
            echo "   Private key format: 0x prefix found ✅"
        fi
        echo "   Hex validation: ✅"

        start=$(echo "$private_key" | cut -c1-7)
        end=$(echo "$private_key" | cut -c62-66)
        echo "   Private key preview: ${start}...${end}"
    else
        echo "   Private key: Using existing secret ✅"
    fi

    unset private_key
    unset l2_rpc_url
    unset hex_part
    unset start
    unset end
    unset has_0x_prefix
    unset update_rpc
    unset update_private_key

    echo ""
    echo "🎉 Environment configuration completed!"
    echo ""
    echo "💡 Next steps:"
    echo "   0. Verify configuration (Optional): $0 verify-env"
    echo "   1. Check overall setup: $0 check"
    echo "   2. Start services: $0 run"
}

verify_env() {
    echo "🔍 Verifying environment configuration..."

    local env_file=".env.${ENVIRONMENT}"

    if [ ! -f "$env_file" ]; then
        echo "❌ $env_file file not found"
        echo "💡 Run: $0 setup first to create the initial $env_file file"
        return 1
    fi

    echo "✅ $env_file file exists"
    echo ""

    local verification_passed=true

    echo "🌐 Checking L2_RPC_URL configuration..."
    local l2_rpc_url=$(grep "^L2_RPC_URL=" "$env_file" 2>/dev/null | cut -d'=' -f2-)

    if [ -z "$l2_rpc_url" ]; then
        echo "❌ L2_RPC_URL not found in $env_file file"
        verification_passed=false
    elif [ "$l2_rpc_url" = "<your-rpc-url>" ]; then
        echo "⚠️  L2_RPC_URL is still set to placeholder value"
        verification_passed=false
    else
        echo "✅ L2_RPC_URL is configured: $l2_rpc_url"

        if echo "$l2_rpc_url" | grep -qE '^https?://[a-zA-Z0-9.-]+'; then
            echo "   Format: Valid URL ✅"

            if echo "$l2_rpc_url" | grep -q "localhost\|127.0.0.1"; then
                echo "   Type: Local endpoint ⚠️"
                echo "   Note: Ensure your local node is accessible from Docker containers"
            elif echo "$l2_rpc_url" | grep -q "^https://"; then
                echo "   Security: HTTPS ✅"
            elif echo "$l2_rpc_url" | grep -q "^http://"; then
                echo "   Security: HTTP ⚠️ (Consider HTTPS for production)"
            fi

            echo ""
            if check_rpc_connectivity "$l2_rpc_url" true; then
                echo "   Connectivity: ✅"
            else
                echo "   Connectivity: ❌"
                verification_passed=false
            fi
        else
            echo "   Format: Invalid URL ❌"
            echo "   Expected: http:// or https:// URL"
            verification_passed=false
        fi
    fi

    echo ""

    echo "🔐 Checking private key configuration..."

    local secret_name="block_builder_private_key_${ENVIRONMENT}"

    if ! docker info 2>/dev/null | grep -q "Swarm: active"; then
        echo "❌ Docker Swarm is not active"
        echo "💡 Run: docker swarm init"
        verification_passed=false
    else
        if docker secret ls 2>/dev/null | grep -q "$secret_name"; then
            echo "✅ Docker secret '$secret_name' exists"

            echo "🔍 Verifying private key content..."

            docker service create \
                --name temp-secret-reader \
                --secret "$secret_name" \
                --detach \
                alpine:latest \
                sleep 30 > /dev/null 2>&1

            sleep 3

            container_id=$(docker ps --filter "label=com.docker.swarm.service.name=temp-secret-reader" --format "{{.ID}}")

            if [ -n "$container_id" ]; then
                private_key_content=$(docker exec "$container_id" cat "/run/secrets/$secret_name" 2>/dev/null)

                if [ -n "$private_key_content" ]; then
                    if echo "$private_key_content" | grep -q "^0x[0-9a-fA-F]\{64\}$"; then
                        echo "   Format: Valid private key ✅"
                        start=$(echo "$private_key_content" | cut -c1-7)
                        end=$(echo "$private_key_content" | cut -c62-66)
                        echo "   Preview: ${start}...${end}"
                    else
                        echo "   Format: Invalid private key ❌"
                        verification_passed=false
                    fi
                else
                    echo "   Content: Empty or inaccessible ❌"
                    verification_passed=false
                fi
            else
                echo "   Verification: Could not access secret content ⚠️"
            fi

            docker service rm temp-secret-reader > /dev/null 2>&1

        else
            echo "❌ Docker secret '$secret_name' not found"
            verification_passed=false
        fi
    fi

    echo ""

    echo "📄 Environment verification summary:"
    echo "   .env file: ✅"

    if [ "$verification_passed" = true ]; then
        echo "   L2_RPC_URL: ✅"
        echo "   Private key: ✅"
        echo ""
        echo "✅ All environment configurations are valid!"

        echo ""
        echo "💡 Next steps:"
        echo "   1. Check overall setup: $0 check"
        echo "   2. Start services: $0 run"

        return 0
    else
        echo "   Configuration: ❌ Issues found"
        echo ""
        echo "❌ Environment verification failed"
        echo "💡 Run: $0 setup-env to configure missing settings"
        return 1
    fi
}

check() {
    show_current_environment

    echo "🔍 Checking required tools..."
    if ! check_required_tools; then
        echo ""
        echo "❌ Required tools are missing. Please install them before proceeding."
        return 1
    fi
    echo ""

    local files_exist=true
    local config_valid=true

    local env_file=".env.${ENVIRONMENT}"

    if [ -f "frpc.toml" ]; then
        echo "✅ frpc.toml exists"
    else
        echo "❌ frpc.toml not found"
        files_exist=false
    fi

    if [ -f "nginx.conf" ]; then
        echo "✅ nginx.conf exists"
    else
        echo "❌ nginx.conf not found"
        files_exist=false
    fi

    if [ -f "docker-compose.yml" ]; then
        echo "✅ docker-compose.yml exists"
    else
        echo "❌ docker-compose.yml not found"
        files_exist=false
    fi

    if [ -f "$env_file" ]; then
        echo "✅ $env_file exists"
        echo ""

        echo "🌐 Checking L2_RPC_URL configuration..."
        local l2_rpc_url=$(grep "^L2_RPC_URL=" "$env_file" 2>/dev/null | cut -d'=' -f2-)

        if [ -z "$l2_rpc_url" ]; then
            echo "❌ L2_RPC_URL not found in $env_file file"
            config_valid=false
        elif [ "$l2_rpc_url" = "<your-rpc-url>" ]; then
            echo "⚠️  L2_RPC_URL is still set to placeholder value"
            echo "   Current value: $l2_rpc_url"
            echo "💡 Run: $0 setup-env to configure L2_RPC_URL"
            config_valid=false
        else
            echo "✅ L2_RPC_URL is configured"
            echo "   Value: $l2_rpc_url"

            if echo "$l2_rpc_url" | grep -qE '^https?://[a-zA-Z0-9.-]+'; then
                echo "   Format: Valid URL ✅"

                if echo "$l2_rpc_url" | grep -q "^https://"; then
                    echo "   Security: HTTPS ✅"
                elif echo "$l2_rpc_url" | grep -q "^http://"; then
                    echo "   Security: HTTP ⚠️"
                fi

                echo ""
                if check_rpc_connectivity "$l2_rpc_url" true; then
                    echo ""
                else
                    config_valid=false
                fi
            else
                echo "   Format: Invalid URL ❌"
                echo "   Expected: http:// or https:// URL"
                config_valid=false
            fi
        fi
        echo ""
    else
        echo "❌ $env_file not found"
        files_exist=false
    fi

    echo "🔐 Checking private key configuration..."
    if ! check_private_key_config; then
        config_valid=false
    fi
    echo ""

    if [ "$files_exist" = false ]; then
        echo "❌ Missing configuration files"
        echo "💡 Run: $0 setup"
        return 1
    fi

    if [ "$config_valid" = false ]; then
        echo "❌ Configuration validation failed"
        echo "💡 Run: $0 setup-env to fix configuration issues"
        return 1
    fi

    if [ -f "frpc.toml" ]; then
        local server_addr=$(grep "serverAddr" frpc.toml | sed 's/serverAddr = "\([^"]*\)"/\1/')
        local uuid=$(grep "locations" frpc.toml | sed 's/.*\/\([^"]*\)".*/\1/')

        if [ -n "$server_addr" ] && [ -n "$uuid" ]; then
            echo "🌐 Your block builder URL: https://${server_addr}/${uuid}"
            echo ""
        fi
    fi

    echo "📄 frpc.toml content:"
    sed 's/\(auth\.token = "\)\([^"]\{5\}\)[^"]*\([^"]\{5\}\)\(".*\)/\1\2...\3\4/' frpc.toml
    echo ""

    echo "🎉 All checks passed! Your configuration is ready:"
    echo "   ✅ Configuration files exist"
    echo "   ✅ L2 RPC URL is configured and accessible"
    echo "   ✅ Private key is configured"
    echo ""
    echo "💡 Next step: $0 run"
}

run() {
    show_current_environment

    if [ ! -f "frpc.toml" ] || [ ! -f "nginx.conf" ]; then
        echo "❌ Configuration files not found"
        echo "Run: $0 setup first"
        return 1
    fi

    echo "🐳 Checking Docker status..."
    if ! docker info >/dev/null 2>&1; then
        echo "❌ Docker is not running or not accessible"
        echo "💡 Please start Docker and try again"
        return 1
    fi

    echo "🔐 Checking private key configuration..."
    if ! check_private_key_config; then
        echo "❌ Cannot start without private key configuration"
        return 1
    fi
    echo ""

    echo "🌐 Checking .env configuration..."
    local env_file=".env.${ENVIRONMENT}"

    if [ ! -f "$env_file" ]; then
        echo "❌ $env_file file not found"
        echo "💡 Run: $0 setup first to create the initial $env_file file"
        return 1
    fi

    local l2_rpc_url=$(grep "^L2_RPC_URL=" "$env_file" 2>/dev/null | cut -d'=' -f2-)

    if [ -z "$l2_rpc_url" ]; then
        echo "❌ L2_RPC_URL not found in $env_file file"
        echo "💡 Run: $0 setup-env to configure L2_RPC_URL"
        return 1
    elif [ "$l2_rpc_url" = "<your-rpc-url>" ]; then
        echo "❌ L2_RPC_URL is still set to placeholder value"
        echo "   Current value: $l2_rpc_url"
        echo "💡 Run: $0 setup-env to configure L2_RPC_URL and private key"
        return 1
    else
        echo "✅ L2_RPC_URL is configured: $l2_rpc_url"
        if ! echo "$l2_rpc_url" | grep -qE '^https?://[a-zA-Z0-9.-]+'; then
            echo "❌ Invalid L2_RPC_URL format"
            echo "   Current value: $l2_rpc_url"
            echo "💡 Run: $0 setup-env to fix L2_RPC_URL configuration"
            return 1
        fi
    fi

    server_addr=$(grep "serverAddr" frpc.toml | sed 's/serverAddr = "\([^"]*\)"/\1/')
    uuid=$(grep "locations" frpc.toml | sed 's/.*\/\([^"]*\)".*/\1/')

    if [ -z "$server_addr" ] || [ -z "$uuid" ]; then
        echo "❌ serverAddr or UUID not found in frpc.toml"
        return 1
    fi

    block_builder_url="https://${server_addr}/${uuid}"

    echo "🚀 Starting Docker Stack..."
    echo "📍 BLOCK_BUILDER_URL: $block_builder_url"
    echo ""
    echo "🩺 Health Check Command:"
    echo "   curl ${block_builder_url}/health-check"
    echo ""

     if ! check_docker_swarm >/dev/null 2>&1; then
        echo "⚠️  Docker Swarm is not active, initializing..."
        docker swarm init
        echo "✅ Docker Swarm initialized"
    fi

    stack_name="block-builder-stack-${ENVIRONMENT}"

    BLOCK_BUILDER_URL="$block_builder_url" docker stack deploy --detach=true -c docker-compose.yml "$stack_name"
    echo "✅ Started successfully as Docker Stack"
    echo ""
    echo "💡 To check health, run: $0 health"
    echo "💡 To monitor the services, run: $0 monitor"
}

stop() {
    if [ ! -f "docker-compose.yml" ]; then
        echo "❌ docker-compose.yml not found"
        echo "Run: $0 setup first"
        return 1
    fi

    if ! confirm_action "Are you sure you want to stop Docker Stack services?"; then
        return 0
    fi

    echo "🛑 Stopping Docker Stack services..."

    echo "🐳 Checking Docker status..."
    if ! docker info >/dev/null 2>&1; then
        echo "❌ Docker is not running or not accessible"
        echo "💡 Please start Docker and try again"
        return 1
    fi

    if ! docker info 2>/dev/null | grep -q "Swarm: active"; then
        echo "⚠️  Docker Swarm is not active, no stack to stop"
        return 0
    fi

    stack_name="block-builder-stack-${ENVIRONMENT}"

    if docker stack ls | grep -q "$stack_name"; then
        docker stack rm "$stack_name"
        echo "✅ Docker Stack '$stack_name' removed successfully"

        echo "⏳ Waiting for services to be completely removed..."
        sleep 5

        while docker service ls | grep -q "$stack_name"; do
            echo "   Still removing services..."
            sleep 2
        done

        echo "✅ All stack services stopped successfully"
    else
        echo "ℹ️  Stack '$stack_name' not found or already stopped"
    fi

    echo ""
    echo "💡 To restart, run: $0 run"
}

health_check() {
    echo "🩺 Testing your block builder health..."

    if ! command -v curl >/dev/null 2>&1; then
        echo "❌ curl not found. Cannot test health check."
        return 1
    fi

    if [ ! -f "frpc.toml" ]; then
        echo "❌ frpc.toml not found"
        echo "💡 Run: $0 setup first to create configuration"
        return 1
    fi

    local server_addr=$(grep "serverAddr" frpc.toml | sed 's/serverAddr = "\([^"]*\)"/\1/')
    local uuid=$(grep "locations" frpc.toml | sed 's/.*\/\([^"]*\)".*/\1/')

    if [ -z "$server_addr" ] || [ -z "$uuid" ]; then
        echo "❌ Could not extract server address or UUID from frpc.toml"
        return 1
    fi

    local block_builder_url="https://${server_addr}/${uuid}"
    local health_endpoint="${block_builder_url}/health-check"
    local fee_info_endpoint="${block_builder_url}/fee-info"
    local indexer_registration_endpoint="${INDEXER_API_ENDPOINT}/v1/indexer/builders/registration"

    echo "🔗 Block Builder URL: $block_builder_url"
    echo "🩺 Testing endpoints..."
    echo ""

    local overall_success=true
    local block_builder_address=""

    echo "1️⃣ Testing health-check endpoint..."
    echo "   URL: $health_endpoint"

    local http_code
    local response_body
    local curl_start_time=$(date +%s)

    if response_body=$(curl -s --connect-timeout 10 --max-time 30 -w "%{http_code}" "$health_endpoint" 2>/dev/null); then
        http_code="${response_body: -3}"
        response_body="${response_body%???}"

        local curl_end_time=$(date +%s)
        local response_time=$((curl_end_time - curl_start_time))

        if [ "$http_code" -eq 200 ]; then
            echo "   ✅ Health check passed (HTTP $http_code, ${response_time}s)"

            if [ -n "$response_body" ]; then
                echo "   📄 Response:"
                if command -v jq >/dev/null 2>&1 && echo "$response_body" | jq empty 2>/dev/null; then
                    echo "$response_body" | jq . | sed 's/^/      /'
                else
                    echo "      $response_body"
                fi
            fi
        else
            echo "   ❌ Health check failed (HTTP $http_code)"
            if [ -n "$response_body" ]; then
                echo "   Response: $response_body"
            fi
            overall_success=false
        fi
    else
        echo "   ❌ Cannot reach health check endpoint"
        overall_success=false
    fi

    echo ""

    echo "2️⃣ Testing fee-info endpoint..."
    echo "   URL: $fee_info_endpoint"

    curl_start_time=$(date +%s)

    if response_body=$(curl -s --connect-timeout 10 --max-time 30 -w "%{http_code}" "$fee_info_endpoint" 2>/dev/null); then
        http_code="${response_body: -3}"
        response_body="${response_body%???}"

        curl_end_time=$(date +%s)
        response_time=$((curl_end_time - curl_start_time))

        if [ "$http_code" -eq 200 ]; then
            echo "   ✅ Fee info endpoint passed (HTTP $http_code, ${response_time}s)"

            if [ -n "$response_body" ]; then
                echo "   📄 Fee Information:"
                if command -v jq >/dev/null 2>&1 && echo "$response_body" | jq empty 2>/dev/null; then
                    echo "$response_body" | jq . | sed 's/^/      /'

                    block_builder_address=$(echo "$response_body" | jq -r '.blockBuilderAddress // empty')

                    if echo "$response_body" | jq -e '.registration_fee' >/dev/null 2>&1; then
                        local reg_fee=$(echo "$response_body" | jq -r '.registration_fee // "N/A"')
                        echo "   💰 Registration Fee: $reg_fee"
                    fi

                    if echo "$response_body" | jq -e '.non_registration_fee' >/dev/null 2>&1; then
                        local non_reg_fee=$(echo "$response_body" | jq -r '.non_registration_fee // "N/A"')
                        echo "   💰 Non-Registration Fee: $non_reg_fee"
                    fi
                else
                    echo "      $response_body"
                fi
            fi
        elif [ "$http_code" -eq 404 ]; then
            echo "   ⚠️  Fee info endpoint not found (HTTP $http_code)"
            echo "   💡 This endpoint might not be implemented yet"
        elif [ "$http_code" -ge 500 ]; then
            echo "   ❌ Fee info endpoint server error (HTTP $http_code)"
            overall_success=false
        else
            echo "   ⚠️  Unexpected response from fee info endpoint (HTTP $http_code)"
            if [ -n "$response_body" ]; then
                echo "   Response: $response_body"
            fi
        fi
    else
        echo "   ❌ Cannot reach fee info endpoint"
        overall_success=false
    fi

    echo ""

    if [ -n "$block_builder_address" ]; then
        echo "3️⃣ Testing indexer registration endpoint..."
        local indexer_endpoint="${indexer_registration_endpoint}/${block_builder_address}"
        echo "   URL: $indexer_endpoint"
        echo "   📍 Block Builder Address: $block_builder_address"

        curl_start_time=$(date +%s)

        if response_body=$(curl -s --connect-timeout 10 --max-time 30 -w "%{http_code}" "$indexer_endpoint" 2>/dev/null); then
            http_code="${response_body: -3}"
            response_body="${response_body%???}"

            curl_end_time=$(date +%s)
            response_time=$((curl_end_time - curl_start_time))

            if [ "$http_code" -eq 200 ]; then
                echo "   ✅ Indexer registration endpoint passed (HTTP $http_code, ${response_time}s)"

                if [ -n "$response_body" ]; then
                    echo "   📄 Registration Information:"
                    if command -v jq >/dev/null 2>&1 && echo "$response_body" | jq empty 2>/dev/null; then
                        echo "$response_body" | jq . | sed 's/^/      /'

                        if echo "$response_body" | jq -e '.isRegistered' >/dev/null 2>&1; then
                            local is_registered=$(echo "$response_body" | jq -r '.isRegistered // "N/A"')
                            echo "   📋 Registration Status: $is_registered"
                        fi

                        if echo "$response_body" | jq -e '.registrationDate' >/dev/null 2>&1; then
                            local reg_date=$(echo "$response_body" | jq -r '.registrationDate // "N/A"')
                            echo "   📅 Registration Date: $reg_date"
                        fi

                        if echo "$response_body" | jq -e '.status' >/dev/null 2>&1; then
                            local status=$(echo "$response_body" | jq -r '.status // "N/A"')
                            echo "   🟢 Status: $status"
                        fi
                    else
                        echo "      $response_body"
                    fi
                fi
            elif [ "$http_code" -eq 404 ]; then
                echo "   ⚠️  Block builder not found in indexer (HTTP $http_code)"
                echo "   💡 This block builder might not be registered yet"
            elif [ "$http_code" -ge 500 ]; then
                echo "   ❌ Indexer registration endpoint server error (HTTP $http_code)"
                overall_success=false
            else
                echo "   ⚠️  Unexpected response from indexer endpoint (HTTP $http_code)"
                if [ -n "$response_body" ]; then
                    echo "   Response: $response_body"
                fi
            fi
        else
            echo "   ❌ Cannot reach indexer registration endpoint"
            overall_success=false
        fi

        echo ""
    else
        echo "3️⃣ Skipping indexer registration check..."
        echo "   ⚠️  Could not extract blockBuilderAddress from fee-info response"
        echo ""
    fi

    if [ "$overall_success" = true ]; then
        echo "🎉 Your block builder is healthy and all endpoints are accessible!"
        echo ""
        if [ -n "$block_builder_address" ]; then
            echo "🏗️  Block Builder Address: $block_builder_address"
            echo ""
        fi
        echo "📋 Endpoint Summary:"
        echo "   ✅ Health Check: $health_endpoint"
        echo "   ✅ Fee Info: $fee_info_endpoint"
        if [ -n "$block_builder_address" ]; then
            echo "   ✅ Indexer Registration: ${indexer_registration_endpoint}/${block_builder_address}"
        fi
        return 0
    else
        echo "⚠️  Some issues detected with your block builder"
        echo ""
        if [ -n "$block_builder_address" ]; then
            echo "🏗️  Block Builder Address: $block_builder_address"
            echo ""
        fi
        echo "📋 Endpoint Summary:"
        echo "   Health Check: $health_endpoint"
        echo "   Fee Info: $fee_info_endpoint"
        if [ -n "$block_builder_address" ]; then
            echo "   Indexer Registration: ${indexer_registration_endpoint}/${block_builder_address}"
        fi
        echo ""
        echo "🔧 Debugging steps:"
        echo "   1. Check if services are running: $0 monitor"
        echo "   2. View service logs: docker service logs -f block-builder-stack-${ENVIRONMENT}_block-builder-${ENVIRONMENT}"
        echo "   3. Restart services if needed: $0 run"
        echo ""
        echo "🌐 Manual testing commands:"
        echo "   curl $health_endpoint"
        echo "   curl $fee_info_endpoint"
        if [ -n "$block_builder_address" ]; then
            echo "   curl ${indexer_registration_endpoint}/${block_builder_address}"
        fi

        return 1
    fi
}

monitor() {
    if [ ! -f "docker-compose.yml" ]; then
        echo "❌ docker-compose.yml not found"
        echo "Run: $0 setup first"
        return 1
    fi

    stack_name="block-builder-stack-${ENVIRONMENT}"

    echo "🐳 Checking Docker status..."
    if ! docker info >/dev/null 2>&1; then
        echo "❌ Docker is not running or not accessible"
        echo "💡 Please start Docker and try again"
        return 1
    fi

    if ! docker info 2>/dev/null | grep -q "Swarm: active"; then
        echo "❌ Docker Swarm is not active"
        return 1
    fi

    if ! docker stack ls | grep -q "$stack_name"; then
        echo "❌ Stack '$stack_name' not found"
        echo "💡 Run: $0 run to start the services"
        return 1
    fi

    echo "📊 Monitoring Docker Stack '$stack_name'..."
    echo ""

    echo "🔍 Stack Services:"
    docker stack services "$stack_name"
    echo ""

    echo "💻 Container Processes:"
    for service in $(docker service ls --filter "label=com.docker.stack.namespace=$stack_name" --format "{{.Name}}"); do
        echo "--- $service ---"
        docker service ps "$service"
        echo ""
    done

    echo "📝 Recent Logs (last 5 lines):"
    echo "--- block-builder logs ---"
    docker service logs --tail 5 "${stack_name}_block-builder-${ENVIRONMENT}" 2>/dev/null || echo "No logs available"
    echo ""

    echo "--- nginx-proxy logs ---"
    docker service logs --tail 5 "${stack_name}_nginx-proxy-${ENVIRONMENT}" 2>/dev/null || echo "No logs available"
    echo ""

    echo "--- frp-client logs ---"
    docker service logs --tail 5 "${stack_name}_frp-client-${ENVIRONMENT}" 2>/dev/null || echo "No logs available"
    echo ""

    server_addr=$(grep "serverAddr" frpc.toml 2>/dev/null | sed 's/serverAddr = "\([^"]*\)"/\1/')
    uuid=$(grep "locations" frpc.toml 2>/dev/null | sed 's/.*\/\([^"]*\)".*/\1/')

    if [ -n "$server_addr" ] && [ -n "$uuid" ]; then
        echo "🌐 Health Check:"
        block_builder_url="https://${server_addr}/${uuid}"
        echo "Testing: ${block_builder_url}/health-check"

        if curl -s --max-time 10 "${block_builder_url}/health-check" >/dev/null 2>&1; then
            echo "✅ Health check passed"
        else
            echo "❌ Health check failed"
        fi
    fi

    echo ""
    echo "💡 Commands:"
    echo "   View live logs: docker service logs -f builder-stack_block-builder"
    echo "   Restart service: $0 run"
    echo "   Health check: $0 health"
    echo "   Stop all: $0 stop"
}

update() {
    echo "🔄 Starting update process..."
    echo ""

    echo "⚠️  This will:"
    echo "   1. Stop all running services"
    echo "   2. Clean up files"
    echo "   3. Download the latest version of this script"
    echo ""

    echo "❓ Do you want to continue? (y/n)"
    echo -n "> "
    read -r continue_choice

    if [ "$continue_choice" != "y" ] && [ "$continue_choice" != "Y" ]; then
        echo "❌ Update cancelled"
        return 1
    fi

    clean

    echo ""
    echo "📥 Downloading latest script..."

    if ! command -v curl >/dev/null 2>&1; then
        echo "❌ curl not found. Please install curl to update."
        return 1
    fi

    if ! validate_api_endpoint "$BUILDER_SCRIPT_URL"; then
        echo "❌ Builder script URL validation failed"
        return 1
    fi

    local temp_script="builder_new.sh"
    local current_script="$0"

    if curl -o "$temp_script" "$BUILDER_SCRIPT_URL" 2>/dev/null; then
        echo "✅ Downloaded latest script"
    else
        echo "❌ Failed to download script"
        echo "   URL: $BUILDER_SCRIPT_URL"
        echo "💡 Please check your internet connection and try again"
        return 1
    fi

    echo ""
    echo "🔍 Validating downloaded script..."

    if [ ! -f "$temp_script" ]; then
        echo "❌ Downloaded file not found"
        return 1
    fi

    if [ ! -s "$temp_script" ]; then
        echo "❌ Downloaded file is empty"
        rm -f "$temp_script"
        return 1
    fi

    if ! head -1 "$temp_script" | grep -q "#!/bin/bash"; then
        echo "❌ Downloaded file doesn't appear to be a valid shell script"
        rm -f "$temp_script"
        return 1
    fi

    echo "✅ Script validation passed"

    echo ""
    echo "🔄 Replacing current script..."

    chmod +x "$temp_script"

    if mv "$temp_script" "$current_script"; then
        echo "✅ Script updated successfully"
    else
        echo "❌ Failed to replace script"
        rm -f "$temp_script"
        return 1
    fi

    echo ""
    echo "📋 Checking preserved configuration..."

    local config_files=("frpc.toml" "nginx.conf" "docker-compose.yml" ".env")
    local found_configs=()

    for file in "${config_files[@]}"; do
        if [ -f "$file" ]; then
            found_configs+=("$file")
        fi
    done

    if [ ${#found_configs[@]} -gt 0 ]; then
        echo "✅ Configuration files preserved:"
        for file in "${found_configs[@]}"; do
            echo "   - $file"
        done
    else
        echo "ℹ️  No configuration files found"
    fi

    echo ""
    echo "🎉 Update completed successfully!"
    echo ""
    echo "💡 What's next:"
    if [ ${#found_configs[@]} -gt 0 ]; then
        echo "   1. Check configuration: $current_script check"
        echo "   2. Restart services: $current_script run"
        echo "   3. Monitor status: $current_script monitor"
    else
        echo "   1. Set up configuration: $current_script setup"
        echo "   2. Configure environment: $current_script setup-env"
        echo "   3. Start services: $current_script run"
    fi
    echo ""
    echo "📖 Check version: $current_script version"
}

docker_clean() {
    echo "🧹 Starting Docker cleanup process..."

    stack_name="block-builder-stack-${ENVIRONMENT}"

    if [ -f "docker-compose.yml" ]; then
        echo "🛑 Stopping Docker Stack services first..."

    if docker info 2>/dev/null | grep -q "Swarm: active"; then
            if docker stack ls | grep -q "$stack_name"; then
                docker stack rm "$stack_name"
                echo "✅ Docker Stack '$stack_name' removed"

                echo "⏳ Waiting for stack services to be completely removed..."
                sleep 5

                while docker service ls | grep -q "$stack_name"; do
                    echo "   Still removing stack services..."
                    sleep 2
                done
                echo "✅ All stack services removed"
            else
                echo "ℹ️  Stack '$stack_name' not found"
            fi
        else
            echo "⚠️  Docker Swarm not active, skipping stack cleanup"
        fi

        echo "🗑️  Removing any remaining related containers..."
        docker rm -f $(docker ps -aq --filter "name=${stack_name}_nginx-proxy-${ENVIRONMENT}" --filter "name=${stack_name}_block-builder-${ENVIRONMENT}" --filter "name=${stack_name}_frp-client-${ENVIRONMENT}") 2>/dev/null || true
        docker rm -f "nginx-proxy-${ENVIRONMENT}" "block-builder-${ENVIRONMENT}" "frp-client-${ENVIRONMENT}" 2>/dev/null || true

        echo "🗑️  Removing Docker images from docker-compose.yml..."

        if command -v docker-compose >/dev/null 2>&1; then
            IMAGES=$(docker-compose config --images 2>/dev/null | grep -v "^$" | sort -u)
        else
            IMAGES=$(grep -E "^\s*image:\s*" docker-compose.yml | sed 's/.*image:\s*//' | sed 's/["\x27]//g' | sort -u)
        fi

        if [ -n "$IMAGES" ]; then
            echo "   Found images in docker-compose.yml:"
            echo "$IMAGES" | while read -r image; do
                if [ -n "$image" ]; then
                    echo "   - $image"
                fi
            done

            echo "$IMAGES" | while read -r image; do
                if [ -n "$image" ]; then
                    if docker rmi "$image" 2>/dev/null; then
                        echo "   ✅ Removed: $image"
                    else
                        echo "   ℹ️  Image not found or still in use: $image"
                    fi
                fi
            done
        else
            echo "   ⚠️  No images found in docker-compose.yml"
        fi

    else
        echo "⚠️  docker-compose.yml not found, skipping cleanup"
        return 1
    fi

    echo "🔐 Removing Docker secrets..."
    local secret_name="block_builder_private_key_${ENVIRONMENT}"

    if docker info 2>/dev/null | grep -q "Swarm: active"; then
        if docker secret ls 2>/dev/null | grep -q "$secret_name"; then
            docker secret rm "$secret_name" 2>/dev/null && echo "   ✅ Removed: $secret_name" || echo "   ⚠️  Failed to remove: $secret_name"
        else
            echo "   ℹ️  No $secret_name secret found"
        fi
    else
        echo "   ⚠️  Docker Swarm not active, skipping secret cleanup"
    fi

    echo "🌐 Removing Docker networks..."
    docker network rm "builder-network-${ENVIRONMENT}" 2>/dev/null && echo "   ✅ Removed: builder-network-${ENVIRONMENT}" || echo "   ℹ️  Network not found or still in use: builder-network-${ENVIRONMENT}"

    echo "✅ Docker cleanup completed"
}

clean() {
    echo "🧹 Starting cleanup process..."

    if ! confirm_action "Are you sure you want to stop Docker Stack services?"; then
        return 0
    fi

    local files_to_remove=("frpc.toml" "nginx.conf" "docker-compose.yml" ".env.${ENVIRONMENT}")
    local files_removed=0

    docker_clean

    echo "📁 Removing configuration files..."
    for file in "${files_to_remove[@]}"; do
        if [ -f "$file" ]; then
            rm -f "$file"
            echo "   ✅ Removed: $file"
            ((files_removed++))
        else
            echo "   ⚠️  Not found: $file"
        fi
    done

    if [ $files_removed -eq 0 ]; then
        echo "ℹ️  No configuration files found to remove"
    else
        echo "✅ Removed $files_removed configuration file(s)"
    fi

    echo ""
    echo "✨ Cleanup process completed!"
    echo "💡 To start fresh, run: $0 setup"
}

version() {
    show_current_environment

    echo "Block Builder Setup Script"

    local version_source=""
    local proxy_domain=""
    local intmax2_version=""

    if [ -f "docker-compose.yml" ]; then
        local docker_image=$(grep -E "^\s*image:\s*" docker-compose.yml | grep "ghcr.io/internetmaximalism/intmax2" | head -1 | sed 's/.*image:\s*//' | sed 's/["\x27]//g' | xargs)

        if [ -n "$docker_image" ]; then
            intmax2_version=$(echo "$docker_image" | sed 's/.*intmax2://' | sed 's/-arm64$//')

            if [ -n "$intmax2_version" ]; then
                version_source="docker-compose.yml"
            else
                echo "⚠️  Could not extract version from docker image"
            fi
        else
            echo "⚠️  Could not find intmax2 image in docker-compose.yml"
        fi

        if [ -f "frpc.toml" ]; then
            proxy_domain=$(grep "serverAddr" frpc.toml | sed 's/serverAddr = "\([^"]*\)"/\1/')
            if [ -n "$proxy_domain" ]; then
            :
            fi
        fi
    fi

    if [ -z "$intmax2_version" ]; then
        if load_config; then
            intmax2_version="$INTMAX2_VERSION"
            proxy_domain="$PROXY_DOMAIN"
        else
            echo "❌ Failed to fetch configuration from API"
            echo "💡 Check your internet connection and try again"
            return 1
        fi
    fi

    echo ""
    echo "📊 Version Information Summary:"
    echo "   INTMAX2 Version: $intmax2_version"
    echo "   Proxy Domain: ${proxy_domain:-'Not available'}"
    echo "   Architecture: $(uname -m)"
    echo "   Expected Docker Image: $(detect_architecture 2>/dev/null || echo 'ghcr.io/internetmaximalism/intmax2:${intmax2_version}')"
}

case "${1:-help}" in
    setup)
        setup
        ;;
    setup-env)
        setup_env
        ;;
    verify-env)
        verify_env
        ;;
    check)
        check
        ;;
    run)
        run
        ;;
    stop)
        stop
        ;;
    health)
        health_check
        ;;
    monitor)
        monitor
        ;;
    update)
        update
        ;;
    docker-clean)
        docker_clean
        ;;
    clean)
        clean
        ;;
    version)
        version
        ;;
    *)
        echo "Usage: $0 {setup|setup-env|verify-env|check|run|stop|monitor|update|clean|docker-clean|version}"
        echo ""
        echo "Commands:"
        echo "  setup        - Create frpc.toml, nginx.conf, docker-compose.yml, and .env with unique UUID"
        echo "  setup-env    - Configure L2_RPC_URL and private key (unified environment setup)"
        echo "  verify-env   - Verify L2_RPC_URL and private key configuration"
        echo "  check        - Check if config files exist and show content"
        echo "  run          - Start Docker Stack with Nginx proxy"
        echo "  stop         - Stop all Docker Stack services"
        echo "  health       - Check health of the Block Builder service"
        echo "  monitor      - Monitor Docker Stack services status and logs"
        echo "  update       - Download and install the latest version of this script"
        echo "  docker-clean - Remove all Docker containers, images, secrets, and networks related to this setup"
        echo "  clean        - Remove all configuration files with Docker cleanup"
        echo "  version      - Show version information"
        echo ""
        echo "Quick start workflow:"
        echo "  1. $0 setup      # Create initial configuration files"
        echo "  2. $0 setup-env  # Configure L2_RPC_URL and private key"
        echo "  3. $0 check      # Verify all configurations"
        echo "  4. $0 run        # Start the services"
        echo "  5. $0 monitor    # Monitor running services"
        echo ""
        echo "Maintenance:"
        echo "  $0 stop          # Stop services"
        echo "  $0 update        # Update to latest version"
        echo "  $0 clean         # Complete cleanup"
        echo ""
        ;;
esac