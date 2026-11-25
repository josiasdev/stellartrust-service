#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
    String,
};

const TTL_THRESHOLD: u32 = 518_400;
const TTL_EXTEND_TO: u32 = 1_036_800;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    AlreadyRegistered = 1,
    NotFound = 2,
    Unauthorized = 3,
    AlreadyRevoked = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttestationRecord {
    pub issuer: Address,
    pub doc_hash: BytesN<32>,
    pub domain: String,
    pub timestamp: u64,
    pub revoked: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredEvent {
    #[topic]
    pub issuer: Address,
    #[topic]
    pub doc_hash: BytesN<32>,
    pub domain: String,
    pub timestamp: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RevokedEvent {
    #[topic]
    pub issuer: Address,
    #[topic]
    pub doc_hash: BytesN<32>,
    pub timestamp: u64,
}

#[contract]
pub struct StellarTrust;

#[contractimpl]
impl StellarTrust {
    pub fn register(
        env: Env,
        issuer: Address,
        doc_hash: BytesN<32>,
        domain: String,
    ) -> Result<(), ContractError> {
        issuer.require_auth();
        if env.storage().persistent().has(&doc_hash) {
            return Err(ContractError::AlreadyRegistered);
        }
        let record = AttestationRecord {
            issuer,
            doc_hash: doc_hash.clone(),
            domain,
            timestamp: env.ledger().timestamp(),
            revoked: false,
        };
        env.storage().persistent().set(&doc_hash, &record);
        env.storage()
            .persistent()
            .extend_ttl(&doc_hash, TTL_THRESHOLD, TTL_EXTEND_TO);
        RegisteredEvent {
            issuer: record.issuer.clone(),
            doc_hash: doc_hash.clone(),
            domain: record.domain.clone(),
            timestamp: record.timestamp,
        }
        .publish(&env);
        Ok(())
    }

    pub fn verify(env: Env, doc_hash: BytesN<32>) -> Option<AttestationRecord> {
        let record: Option<AttestationRecord> = env.storage().persistent().get(&doc_hash);
        if record.is_some() {
            env.storage()
                .persistent()
                .extend_ttl(&doc_hash, TTL_THRESHOLD, TTL_EXTEND_TO);
        }
        record
    }

    pub fn revoke(env: Env, issuer: Address, doc_hash: BytesN<32>) -> Result<(), ContractError> {
        issuer.require_auth();
        let mut record: AttestationRecord = env
            .storage()
            .persistent()
            .get(&doc_hash)
            .ok_or(ContractError::NotFound)?;
        if record.issuer != issuer {
            return Err(ContractError::Unauthorized);
        }
        if record.revoked {
            return Err(ContractError::AlreadyRevoked);
        }
        record.revoked = true;
        env.storage().persistent().set(&doc_hash, &record);
        env.storage()
            .persistent()
            .extend_ttl(&doc_hash, TTL_THRESHOLD, TTL_EXTEND_TO);
        RevokedEvent {
            issuer: record.issuer.clone(),
            doc_hash: doc_hash.clone(),
            timestamp: record.timestamp,
        }
        .publish(&env);
        Ok(())
    }
}

#[cfg(test)]
mod test;
