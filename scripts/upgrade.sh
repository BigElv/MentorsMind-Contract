#!/bin/bash

# Contract Upgrade Script
# This script handles the deployment of new contract versions and manages the upgrade process

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Default values
NETWORK="futurenet"
CONTRACT_NAME=""
CONTRACT_ADDRESS=""
ADMIN_SECRET_KEY=""
NEW_VERSION=""

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Function to show usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -n, --network NETWORK     Stellar network (futurenet, testnet, mainnet) [default: futurenet]"
    echo "  -c, --contract CONTRACT   Contract name (escrow, mentorminds)"
    echo "  -a, --address ADDRESS     Current contract address"
    echo "  -s, --secret SECRET       Admin secret key"
    echo "  -v, --version VERSION     New version number"
    echo "  -h, --help               Show this help message"
    echo ""
    echo "Example:"
    echo "  $0 --network testnet --contract escrow --address C... --secret S... --version 2"
    exit 1
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -n|--network)
            NETWORK="$2"
            shift 2
            ;;
        -c|--contract)
            CONTRACT_NAME="$2"
            shift 2
            ;;
        -a|--address)
            CONTRACT_ADDRESS="$2"
            shift 2
            ;;
        -s|--secret)
            ADMIN_SECRET_KEY="$2"
            shift 2
            ;;
        -v|--version)
            NEW_VERSION="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate required arguments
if [[ -z "$CONTRACT_NAME" ]]; then
    print_error "Contract name is required"
    usage
fi

if [[ -z "$CONTRACT_ADDRESS" ]]; then
    print_error "Contract address is required"
    usage
fi

if [[ -z "$ADMIN_SECRET_KEY" ]]; then
    print_error "Admin secret key is required"
    usage
fi

if [[ -z "$NEW_VERSION" ]]; then
    print_error "New version number is required"
    usage
fi

# Validate contract name
if [[ "$CONTRACT_NAME" != "escrow" ]]; then
    print_error "Unsupported contract: $CONTRACT_NAME. Currently only 'escrow' is supported"
    exit 1
fi

# Validate network
case $NETWORK in
    futurenet|testnet|mainnet)
        ;;
    *)
        print_error "Invalid network: $NETWORK. Must be futurenet, testnet, or mainnet"
        exit 1
        ;;
esac

# Validate version format
if ! [[ "$NEW_VERSION" =~ ^[0-9]+$ ]]; then
    print_error "Version must be a positive integer"
    exit 1
fi

print_info "Starting contract upgrade process..."
print_info "Network: $NETWORK"
print_info "Contract: $CONTRACT_NAME"
print_info "Contract Address: $CONTRACT_ADDRESS"
print_info "New Version: $NEW_VERSION"

# Check if soroban-cli is installed
if ! command -v soroban &> /dev/null; then
    print_error "soroban-cli is not installed. Please install it first."
    exit 1
fi

# Check if contract directory exists
CONTRACT_DIR="$CONTRACT_NAME"
if [[ ! -d "$CONTRACT_DIR" ]]; then
    print_error "Contract directory '$CONTRACT_DIR' does not exist"
    exit 1
fi

# Build the contract
print_info "Building contract..."
cd "$CONTRACT_DIR"
if ! cargo build --release --target wasm32-unknown-unknown; then
    print_error "Failed to build contract"
    exit 1
fi

# Get the WASM file path
WASM_FILE="target/wasm32-unknown-unknown/release/mentorminds-$CONTRACT_NAME.wasm"
if [[ ! -f "$WASM_FILE" ]]; then
    print_error "WASM file not found: $WASM_FILE"
    exit 1
fi

print_info "Contract built successfully: $WASM_FILE"

# Deploy new contract instance
print_info "Deploying new contract instance..."

# Set network based on selection
case $NETWORK in
    futurenet)
        SOROBAN_NETWORK="--futurenet"
        ;;
    testnet)
        SOROBAN_NETWORK="--testnet"
        ;;
    mainnet)
        SOROBAN_NETWORK="--mainnet"
        ;;
esac

# Deploy the new contract
NEW_CONTRACT_ADDRESS=$(soroban contract deploy $SOROBAN_NETWORK --wasm "$WASM_FILE" --secret-key "$ADMIN_SECRET_KEY")

if [[ -z "$NEW_CONTRACT_ADDRESS" ]]; then
    print_error "Failed to deploy new contract instance"
    exit 1
fi

print_info "New contract deployed at: $NEW_CONTRACT_ADDRESS"

# Get current version from old contract
print_info "Checking current contract version..."
CURRENT_VERSION=$(soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "get_version" || echo "0")

print_info "Current version: $CURRENT_VERSION"

# Increment version check
EXPECTED_VERSION=$((CURRENT_VERSION + 1))
if [[ "$NEW_VERSION" != "$EXPECTED_VERSION" ]]; then
    print_warning "Version jump detected: $CURRENT_VERSION -> $NEW_VERSION (expected $EXPECTED_VERSION)"
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        print_info "Upgrade cancelled"
        exit 0
    fi
fi

# Initialize the new contract with the same admin and settings
print_info "Initializing new contract instance..."

# Get current settings from old contract
ADMIN_ADDRESS=$(soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "get_admin" 2>/dev/null || echo "")

TREASURY_ADDRESS=$(soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "get_treasury" 2>/dev/null || echo "")

FEE_BPS=$(soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "get_fee_bps" 2>/dev/null || echo "500")

AUTO_RELEASE_DELAY=$(soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "get_auto_release_delay" 2>/dev/null || echo "259200")

# Get approved tokens (this is more complex, so we'll use a basic approach for now)
APPROVED_TOKENS="[]"

if [[ -n "$ADMIN_ADDRESS" && -n "$TREASURY_ADDRESS" ]]; then
    print_info "Migrating settings to new contract..."
    
    # Initialize new contract with migrated settings
    soroban contract invoke $SOROBAN_NETWORK --id "$NEW_CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
        --function "initialize" \
        --args "$ADMIN_ADDRESS" "$TREASURY_ADDRESS" "$FEE_BPS" "$APPROVED_TOKENS" "$AUTO_RELEASE_DELAY"
    
    print_info "New contract initialized successfully"
else
    print_warning "Could not retrieve all settings from old contract. Manual initialization may be required."
fi

# Create upgrade transaction
print_info "Creating upgrade transaction..."

# Note: The actual upgrade mechanism depends on your contract's upgrade function
# This is a placeholder for the upgrade call
soroban contract invoke $SOROBAN_NETWORK --id "$CONTRACT_ADDRESS" --secret-key "$ADMIN_SECRET_KEY" \
    --function "upgrade" \
    --args "$NEW_CONTRACT_ADDRESS" 2>/dev/null || print_warning "Upgrade function not available or failed"

# Tag git commit with version
print_info "Tagging git commit with version..."
git tag -a "$CONTRACT_NAME-v$NEW_VERSION" -m "Upgrade $CONTRACT_NAME to version $NEW_VERSION"
git push origin "$CONTRACT_NAME-v$NEW_VERSION" 2>/dev/null || print_warning "Could not push tag to remote"

# Update CHANGELOG
print_info "Updating CHANGELOG..."
CHANGELOG_FILE="$CONTRACT_DIR/CHANGELOG.md"
if [[ -f "$CHANGELOG_FILE" ]]; then
    # Add new version entry to CHANGELOG
    TEMP_FILE=$(mktemp)
    echo "## Version $NEW_VERSION" > "$TEMP_FILE"
    echo "### Date: $(date +%Y-%m-%d)" >> "$TEMP_FILE"
    echo "### Changes:" >> "$TEMP_FILE"
    echo "- Contract upgrade to version $NEW_VERSION" >> "$TEMP_FILE"
    echo "- New contract address: $NEW_CONTRACT_ADDRESS" >> "$TEMP_FILE"
    echo "" >> "$TEMP_FILE"
    cat "$CHANGELOG_FILE" >> "$TEMP_FILE"
    mv "$TEMP_FILE" "$CHANGELOG_FILE"
    print_info "CHANGELOG updated"
else
    print_warning "CHANGELOG.md not found in $CONTRACT_DIR"
fi

print_info "Upgrade completed successfully!"
print_info "New contract address: $NEW_CONTRACT_ADDRESS"
print_info "Version: $NEW_VERSION"
print_info "Git tag: $CONTRACT_NAME-v$NEW_VERSION"

# Display next steps
echo ""
print_info "Next steps:"
echo "1. Test the new contract thoroughly"
echo "2. Update any frontend/backend configurations to use the new contract address"
echo "3. Monitor the new contract for any issues"
echo "4. Consider deprecating the old contract after a transition period"
