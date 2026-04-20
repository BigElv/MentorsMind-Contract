#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env};

#[test]
fn test_kyc_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let user = Address::generate(&env);
    let contract_id = env.register_contract(None, KycRegistry);
    let client = KycRegistryClient::new(&env, &contract_id);

    client.initialize(&admin);

    let provider_hash = BytesN::from_array(&env, &[0; 32]);
    let expiry = 1000;

    // Initially no KYC
    assert_eq!(client.get_kyc_level(&user), KycLevel::None);
    assert!(!client.is_kyc_valid(&user, &KycLevel::Basic));

    // Set KYC level
    client.set_kyc_level(&admin, &user, &KycLevel::Basic, &expiry, &provider_hash);
    assert_eq!(client.get_kyc_level(&user), KycLevel::Basic);
    assert!(client.is_kyc_valid(&user, &KycLevel::Basic));
    assert!(!client.is_kyc_valid(&user, &KycLevel::Enhanced));

    // Test expiry
    env.ledger().set_timestamp(1001);
    assert_eq!(client.get_kyc_level(&user), KycLevel::None);
    assert!(!client.is_kyc_valid(&user, &KycLevel::Basic));

    // Reset with longer expiry
    env.ledger().set_timestamp(0);
    client.set_kyc_level(
        &admin,
        &user,
        &KycLevel::Institutional,
        &5000,
        &provider_hash,
    );
    assert_eq!(client.get_kyc_level(&user), KycLevel::Institutional);
    assert!(client.is_kyc_valid(&user, &KycLevel::Basic));
    assert!(client.is_kyc_valid(&user, &KycLevel::Institutional));

    // Revoke
    client.revoke_kyc(&admin, &user);
    assert_eq!(client.get_kyc_level(&user), KycLevel::None);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, KycRegistry);
    let client = KycRegistryClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.initialize(&admin);
}
