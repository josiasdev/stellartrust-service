#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn setup(env: &Env) -> (Address, StellarTrustClient<'_>) {
    let contract_id = env.register(StellarTrust, ());
    let client = StellarTrustClient::new(env, &contract_id);
    (contract_id, client)
}

fn doc_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn register_and_verify() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client) = setup(&env);
    let issuer = Address::generate(&env);
    let hash = doc_hash(&env, 1);
    let domain = String::from_str(&env, "ufc-certificados");

    client.register(&issuer, &hash, &domain);

    let record = client.verify(&hash).expect("record should exist");
    assert_eq!(record.issuer, issuer);
    assert_eq!(record.doc_hash, hash);
    assert_eq!(record.domain, domain);
    assert_eq!(record.revoked, false);
    assert_eq!(record.timestamp, env.ledger().timestamp());
}

#[test]
fn verify_unknown_returns_none() {
    let env = Env::default();
    let (_id, client) = setup(&env);
    let hash = doc_hash(&env, 9);

    assert_eq!(client.verify(&hash), None);
}

#[test]
fn revoke_marks_revoked() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client) = setup(&env);
    let issuer = Address::generate(&env);
    let hash = doc_hash(&env, 2);
    let domain = String::from_str(&env, "logistica-reversa");

    client.register(&issuer, &hash, &domain);
    client.revoke(&issuer, &hash);

    let record = client.verify(&hash).expect("record should exist");
    assert!(record.revoked);
}

#[test]
fn duplicate_register_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client) = setup(&env);
    let issuer = Address::generate(&env);
    let hash = doc_hash(&env, 3);
    let domain = String::from_str(&env, "ufc-certificados");

    client.register(&issuer, &hash, &domain);
    let err = client.try_register(&issuer, &hash, &domain);
    assert_eq!(err, Err(Ok(ContractError::AlreadyRegistered)));
}

#[test]
fn unauthorized_revoke_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (_id, client) = setup(&env);
    let issuer = Address::generate(&env);
    let other = Address::generate(&env);
    let hash = doc_hash(&env, 4);
    let domain = String::from_str(&env, "ufc-certificados");

    client.register(&issuer, &hash, &domain);
    let err = client.try_revoke(&other, &hash);
    assert_eq!(err, Err(Ok(ContractError::Unauthorized)));
}
