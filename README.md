# MentorMinds Stellar - Smart Contracts

Soroban smart contracts for the MentorMinds Stellar platform, providing secure escrow and payment functionality on the Stellar blockchain.

## 🚀 Overview

This repository contains the Soroban smart contracts that power the MentorMinds platform:

- **Escrow Contract**: Secure payment escrow for mentoring sessions
- **Multi-Sig Wallet**: Multi-signature wallet for platform administration
- **Payment Router**: Automated payment distribution and fee collection

## 📋 Prerequisites

- **Rust** 1.70+ with wasm32 target
- **Soroban CLI** (latest version)
- **Stellar Account** (testnet for development)
- **Node.js** 18+ (for testing scripts)

## 🛠️ Installation

### 1. Install Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
```

### 2. Install Soroban CLI
```bash
cargo install --locked soroban-cli
```

### 3. Configure Soroban for Testnet
```bash
soroban config network add testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"
```

### 4. Create Identity
```bash
soroban config identity generate default
```

### 5. Fund Your Account
```bash
soroban config identity address default
# Use the Stellar Laboratory to fund your testnet account
# https://laboratory.stellar.org/#account-creator?network=test
```

## 📁 Project Structure

```
mentorminds-contracts/
├── escrow/                 # Escrow smart contract
│   ├── src/
│   │   └── lib.rs         # Main contract code
│   ├── Cargo.toml
│   └── README.md
├── multisig/              # Multi-signature wallet
│   ├── src/
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── README.md
├── payment-router/        # Payment distribution
│   ├── src/
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── README.md
├── scripts/               # Deployment and testing scripts
│   ├── deploy.sh
│   ├── test.sh
│   └── invoke.sh
├── tests/                 # Integration tests
└── README.md
```

## 🏗️ Contracts

### 1. Escrow Contract

Manages secure payment escrow for mentoring sessions.

**Features**:
- Lock funds until session completion
- Automatic release on confirmation
- Dispute resolution mechanism
- Time-based auto-release
- Refund support for cancellations

**Functions**:
- `create_escrow(mentor, learner, amount, session_id)`
- `release_funds(escrow_id)`
- `dispute(escrow_id, reason)`
- `resolve_dispute(escrow_id, decision)`
- `refund(escrow_id)`

### 2. Multi-Sig Wallet

Multi-signature wallet for platform administration.

**Features**:
- Configurable signers and threshold
- Transaction proposal and approval
- Time-lock for delayed execution
- Emergency recovery procedures

**Functions**:
- `add_signer(address, weight)`
- `remove_signer(address)`
- `propose_transaction(to, amount, data)`
- `approve_transaction(tx_id)`
- `execute_transaction(tx_id)`

### 3. Payment Router

Automated payment distribution and fee collection.

**Features**:
- Automatic fee calculation
- Multi-recipient payments
- Asset conversion support
- Payment batching

**Functions**:
- `route_payment(from, to, amount, fee_percentage)`
- `batch_payments(payments[])`
- `calculate_fees(amount)`

## 🔨 Building Contracts

### Build All Contracts
```bash
./scripts/build-all.sh
```

### Build Individual Contract
```bash
cd escrow
cargo build --target wasm32-unknown-unknown --release
```

### Optimize WASM
```bash
soroban contract optimize --wasm target/wasm32-unknown-unknown/release/escrow.wasm
```

## 🚀 Deployment

### Deploy to Testnet
```bash
# Deploy escrow contract
cd escrow
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/escrow.wasm \
  --source default \
  --network testnet

# Save contract ID
export ESCROW_CONTRACT_ID=<contract-id>
```

### Initialize Contract
```bash
soroban contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source default \
  --network testnet \
  -- initialize \
  --admin <admin-address> \
  --platform_fee 5
```

## 🧪 Testing

### Run Unit Tests
```bash
cd escrow
cargo test
```

### Run Integration Tests
```bash
./scripts/test-integration.sh
```

### Invoke Contract Functions
```bash
# Create escrow
soroban contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --source default \
  --network testnet \
  -- create_escrow \
  --mentor <mentor-address> \
  --learner <learner-address> \
  --amount 100 \
  --session_id "session-123"
```

## 📝 Development Workflow

1. **Write Contract**: Implement contract logic in Rust
2. **Test Locally**: Run unit tests with `cargo test`
3. **Build**: Compile to WASM with `cargo build`
4. **Optimize**: Optimize WASM size
5. **Deploy to Testnet**: Deploy and test on testnet
6. **Integration Test**: Test with backend API
7. **Audit**: Security audit before mainnet
8. **Deploy to Mainnet**: Final deployment

## 🔐 Security Considerations

- **Access Control**: Proper authorization checks
- **Reentrancy Protection**: Guard against reentrancy attacks
- **Integer Overflow**: Use checked arithmetic
- **Input Validation**: Validate all inputs
- **Emergency Pause**: Implement pause mechanism
- **Upgrade Path**: Plan for contract upgrades

## 🔁 Upgrade Procedure

- Build the new version to WASM
  ```bash
  cd escrow
  cargo build --target wasm32-unknown-unknown --release
  ```
- Upload the new WASM artifact
  ```bash
  soroban contract upload \
    --source default \
    --network testnet \
    --wasm target/wasm32-unknown-unknown/release/escrow.wasm
  ```
- Invoke the contract’s admin-only upgrade entrypoint to switch code at the same contract ID
  - Use the on-chain upgrade mechanism recommended in Soroban’s guide
  - Do not call initialize again; the initialization guard prevents re-initialization
- Verify existing escrows remain readable and new fields default correctly
  - `dispute_reason` defaults to empty
  - `resolved_at` defaults to `0`
  - `auto_release_delay` uses the configured stored value or the 72h default
- Validate new functions on old records
  - Call `dispute`, `resolve_dispute`, and `try_auto_release` on pre-upgrade escrows

Reference: Upgrading Wasm bytecode for a deployed contract (Stellar Docs)

## 📊 Gas Optimization

- Minimize storage operations
- Use efficient data structures
- Batch operations when possible
- Optimize WASM size
- Cache frequently accessed data

## 🔍 Monitoring

### Check Contract Balance
```bash
soroban contract invoke \
  --id $ESCROW_CONTRACT_ID \
  --network testnet \
  -- get_balance
```

### View Contract Events
```bash
soroban events --id $ESCROW_CONTRACT_ID --network testnet
```

## 📚 Resources

- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Soroban Examples](https://github.com/stellar/soroban-examples)
- [Stellar Laboratory](https://laboratory.stellar.org/)
- [Soroban Discord](https://discord.gg/stellardev)

## 🚧 Development Status

- [x] Project setup
- [ ] Escrow contract implementation
- [ ] Multi-sig wallet implementation
- [ ] Payment router implementation
- [ ] Unit tests
- [ ] Integration tests
- [ ] Security audit
- [ ] Testnet deployment
- [ ] Mainnet deployment

## 🤝 Contributing

1. Fork the repository
2. Create a feature branch
3. Write tests for new features
4. Ensure all tests pass
5. Submit a pull request

## 📄 License

MIT License - see LICENSE file for details

## 🆘 Support

For issues and questions:
- Create an issue on GitHub
- Join Stellar Discord
- Check Soroban documentation

---

**Status**: 🟡 In Development

Built with Rust and Soroban for the Stellar blockchain
