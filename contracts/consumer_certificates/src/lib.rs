#![no_std]

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

mod stellartrust {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/stellartrust.wasm");
}

#[contract]
pub struct ConsumerCertificates;

#[contractimpl]
impl ConsumerCertificates {
    pub fn issue_certificate(
        env: Env,
        trust_contract: Address,
        issuer: Address,
        doc_hash: BytesN<32>,
    ) {
        let client = stellartrust::Client::new(&env, &trust_contract);
        let domain = String::from_str(&env, "ufc-certificates");
        client.register(&issuer, &doc_hash, &domain);
    }

    pub fn validate_certificate(
        env: Env,
        trust_contract: Address,
        doc_hash: BytesN<32>,
    ) -> Option<stellartrust::AttestationRecord> {
        let client = stellartrust::Client::new(&env, &trust_contract);
        client.verify(&doc_hash)
    }
}

#[cfg(test)]
mod test;
