#![no_std]

use soroban_sdk::{contract, contractimpl, Bytes, BytesN, Env};

#[contract]
pub struct MockProofVerifier;

#[contractimpl]
impl MockProofVerifier {
    pub fn verify(_env: Env, _proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
        proof == Bytes::from_array(proof.env(), &statement_hash.to_array())
    }
}
