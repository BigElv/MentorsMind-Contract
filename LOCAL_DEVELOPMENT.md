# 🚀 Local Development Environment Setup

This directory contains everything you need to set up a complete local Soroban development environment for MentorMinds contracts.

## 📋 Prerequisites

- **Docker Desktop** (for Windows/Mac) or **Docker + Docker Compose** (for Linux)
- **Node.js** 18+
- **Rust** 1.70+ with `wasm32-unknown-unknown` target
- **Soroban CLI** (latest version)

### Quick Install Commands

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# Install Soroban CLI
cargo install --locked soroban-cli

# Verify Docker installation
docker --version
docker-compose --version
```

## 🎯 Quick Start

### Windows Users

```cmd
# Start the local environment
scripts\setup-local.bat

# Seed with sample data (optional)
npm run local:seed

# Stop the environment
npm run local:stop
```

### macOS/Linux Users

```bash
# Start the local environment
npm run local:start

# Seed with sample data (optional)
npm run local:seed

# Stop the environment
npm run local:stop
```

## 🛠️ Available Commands

```bash
# Environment management
npm run local:start      # Start local Stellar node and deploy contracts
npm run local:stop       # Stop the local Stellar node
npm run local:reset      # Reset the entire environment
npm run local:seed       # Create sample data for testing
npm run local:status     # Check environment status
npm run local:logs       # View Stellar container logs

# Contract development
npm run build            # Build all contracts
npm run build:escrow     # Build only escrow contract
npm run optimize         # Optimize WASM files
npm run test            # Run all tests
npm run clean           # Clean build artifacts
```

## 🌐 Service Endpoints

When running, these services are available:

- **Horizon API**: http://localhost:8000
- **Stellar RPC**: http://localhost:8001  
- **Friendbot** (funding): http://localhost:8002
- **Soroban RPC**: http://localhost:8003

## 👥 Test Accounts

The setup creates 5 pre-funded accounts:

| Account | Role | Purpose |
|---------|------|---------|
| admin | Platform administration | Contract management |
| mentor1 | Web Development mentor | Testing mentor features |
| mentor2 | Smart Contract mentor | Testing advanced features |
| learner1 | Test learner | Testing learner features |
| learner2 | Test learner | Testing dispute scenarios |

Account details are saved in `deployed/accounts.json`.

## 📁 Generated Files

- `deployed/local.json` - Contract deployment addresses
- `deployed/accounts.json` - Test account information  
- `deployed/seed_escrows.json` - Sample escrow IDs (after seeding)
- `deployed/seed_summary.json` - Complete seeding summary (after seeding)

## 🧪 Testing with Sample Data

After running `npm run local:seed`, you'll have:

- **3 escrows** (2 completed, 1 disputed)
- **2 verified mentors** with different skill sets
- **2 completed sessions** with reviews
- **1 active dispute** for testing
- **Oracle price feeds** for XLM and metrics

### Useful Test Commands

```bash
# Check escrow details
soroban contract invoke \
  --id $(jq -r '.contracts.escrow.contract_id' deployed/local.json) \
  --network standalone \
  -- get_escrow \
  --escrow_id <ESCROW_ID>

# View mentor reviews  
soroban contract invoke \
  --id $(jq -r '.contracts.verification.contract_id' deployed/local.json) \
  --network standalone \
  -- get_mentor_reviews \
  --mentor <MENTOR_ADDRESS>

# Check oracle prices
soroban contract invoke \
  --id $(jq -r '.contracts.oracle.contract_id' deployed/local.json) \
  --network standalone \
  -- get_all_prices
```

## 🔧 Troubleshooting

### Common Issues

1. **Docker not running**: Start Docker Desktop first
2. **Port conflicts**: Ensure ports 8000-8003 are available
3. **Permission errors**: Use PowerShell as Administrator (Windows)
4. **Soroban CLI version**: Update to latest version
5. **Build failures**: Run `rustup update` and ensure wasm32 target is installed

### Environment Reset

```bash
# Complete reset
npm run local:reset

# Manual cleanup
npm run local:stop
docker volume rm mentorminds-contract_stellar_data
rm -rf deployed/*.json
```

### Windows-Specific Issues

- Use **PowerShell** or **Command Prompt** as Administrator
- If Docker Desktop shows permission errors, restart it
- For script execution issues, run: `Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser`

## 📚 Next Steps

1. **Explore contracts**: Read the contract source code in `contracts/` and `escrow/`
2. **Run tests**: `npm run test` to execute unit tests
3. **Make changes**: Modify contract code and redeploy with `npm run local:reset`
4. **Create PR**: Follow the contribution guidelines in `CONTRIBUTING.md`

## 🆘 Getting Help

- Check [CONTRIBUTING.md](CONTRIBUTING.md) for detailed documentation
- Create an issue on GitHub for bugs or feature requests
- Join the Stellar Discord for community support
- Review Soroban documentation for contract development

---

🎉 **You're ready to start developing!** The local environment provides everything you need to build and test MentorMinds smart contracts offline.
