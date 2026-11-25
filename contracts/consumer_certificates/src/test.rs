#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String};

fn doc_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn issue_and_validate_certificate_cross_contract() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let trust_contract_id = env.register(stellartrust::WASM, ());
    let consumer_id = env.register(ConsumerCertificates, ());
    let client = ConsumerCertificatesClient::new(&env, &consumer_id);

    let issuer = Address::generate(&env);
    let hash = doc_hash(&env, 7);

    client.issue_certificate(&trust_contract_id, &issuer, &hash);

    let record = client
        .validate_certificate(&trust_contract_id, &hash)
        .expect("certificate should exist");
    assert_eq!(record.issuer, issuer);
    assert_eq!(record.doc_hash, hash);
    assert_eq!(record.domain, String::from_str(&env, "ufc-certificates"));
    assert!(!record.revoked);
}

#[test]
fn validate_unknown_certificate_returns_none() {
    let env = Env::default();
    let trust_contract_id = env.register(stellartrust::WASM, ());
    let consumer_id = env.register(ConsumerCertificates, ());
    let client = ConsumerCertificatesClient::new(&env, &consumer_id);

    let hash = doc_hash(&env, 9);
    assert_eq!(client.validate_certificate(&trust_contract_id, &hash), None);
}
