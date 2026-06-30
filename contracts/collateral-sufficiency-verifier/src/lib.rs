#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype,
    crypto::bn254::{
        Bn254G1Affine, Bn254G2Affine, Fr, BN254_G1_SERIALIZED_SIZE, BN254_G2_SERIALIZED_SIZE,
    },
    vec, Bytes, BytesN, Env, Vec,
};

const PROOF_TYPE_COLLATERAL_SUFFICIENCY: u32 = 2;
const STATEMENT_HASH_HI_INDEX: u32 = 3;
const STATEMENT_HASH_LO_INDEX: u32 = 4;
const MIN_PUBLIC_SIGNAL_COUNT: u32 = 5;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    VerificationKey,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    Truncated = 1,
    MalformedPublicSignals = 2,
}

#[derive(Clone)]
#[contracttype]
pub struct VerificationKey {
    pub alpha: Bn254G1Affine,
    pub beta: Bn254G2Affine,
    pub gamma: Bn254G2Affine,
    pub delta: Bn254G2Affine,
    pub ic: Vec<Bn254G1Affine>,
}

impl VerificationKey {
    pub fn from_bytes(env: &Env, bytes: &Bytes) -> Result<Self, DecodeError> {
        let mut pos = 0u32;
        let alpha = Bn254G1Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G1_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let beta = Bn254G2Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G2_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let gamma = Bn254G2Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G2_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let delta = Bn254G2Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G2_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let ic_len = u32::from_be_bytes(take_array::<4>(bytes, &mut pos)?);
        let mut ic = Vec::new(env);
        for _ in 0..ic_len {
            ic.push_back(Bn254G1Affine::from_bytes(BytesN::from_array(
                env,
                &take_array::<BN254_G1_SERIALIZED_SIZE>(bytes, &mut pos)?,
            )));
        }
        Ok(Self {
            alpha,
            beta,
            gamma,
            delta,
            ic,
        })
    }
}

#[derive(Clone)]
#[contracttype]
pub struct Proof {
    pub a: Bn254G1Affine,
    pub b: Bn254G2Affine,
    pub c: Bn254G1Affine,
}

impl Proof {
    pub fn from_bytes(env: &Env, bytes: &Bytes) -> Result<Self, DecodeError> {
        let mut pos = 0u32;
        let a = Bn254G1Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G1_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let b = Bn254G2Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G2_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        let c = Bn254G1Affine::from_bytes(BytesN::from_array(
            env,
            &take_array::<BN254_G1_SERIALIZED_SIZE>(bytes, &mut pos)?,
        ));
        Ok(Self { a, b, c })
    }
}

#[derive(Clone)]
pub struct ProofPayload {
    pub proof: Proof,
    pub public_signals: Vec<Fr>,
}

pub fn decode_proof_payload(env: &Env, bytes: &Bytes) -> Result<ProofPayload, DecodeError> {
    let proof_size = (BN254_G1_SERIALIZED_SIZE * 2 + BN254_G2_SERIALIZED_SIZE) as u32;
    if bytes.len() < proof_size + 4 {
        return Err(DecodeError::Truncated);
    }

    let proof = Proof::from_bytes(env, &bytes.slice(0..proof_size))?;
    let mut pos = proof_size;
    let signal_len = u32::from_be_bytes(take_array::<4>(bytes, &mut pos)?);
    if signal_len < MIN_PUBLIC_SIGNAL_COUNT {
        return Err(DecodeError::MalformedPublicSignals);
    }

    let remaining = bytes.len().saturating_sub(pos);
    if remaining != signal_len * 32 {
        return Err(DecodeError::MalformedPublicSignals);
    }

    let mut public_signals = Vec::new(env);
    for _ in 0..signal_len {
        public_signals.push_back(Fr::from_bytes(BytesN::from_array(
            env,
            &take_array::<32>(bytes, &mut pos)?,
        )));
    }

    Ok(ProofPayload {
        proof,
        public_signals,
    })
}

pub fn verify_groth16(
    env: &Env,
    vk: &VerificationKey,
    proof: &Proof,
    public_signals: &Vec<Fr>,
) -> bool {
    if public_signals.len() + 1 != vk.ic.len() {
        return false;
    }

    let bn254 = env.crypto().bn254();
    let mut vk_x = vk.ic.get(0).unwrap();
    for (signal, ic_point) in public_signals.iter().zip(vk.ic.iter().skip(1)) {
        let product = bn254.g1_mul(&ic_point, &signal);
        vk_x = bn254.g1_add(&vk_x, &product);
    }

    bn254.pairing_check(
        vec![env, -proof.a.clone(), vk.alpha.clone(), vk_x, proof.c.clone()],
        vec![
            env,
            proof.b.clone(),
            vk.beta.clone(),
            vk.gamma.clone(),
            vk.delta.clone(),
        ],
    )
}

pub fn statement_hash_matches(public_signals: &Vec<Fr>, statement_hash: &BytesN<32>) -> bool {
    if public_signals.len() <= STATEMENT_HASH_LO_INDEX {
        return false;
    }

    let expected = statement_hash.to_array();
    limb_matches(&public_signals.get(STATEMENT_HASH_HI_INDEX).unwrap(), &expected[..16])
        && limb_matches(&public_signals.get(STATEMENT_HASH_LO_INDEX).unwrap(), &expected[16..])
}

fn limb_matches(signal: &Fr, expected: &[u8]) -> bool {
    let bytes = signal.to_bytes().to_array();
    bytes[..16].iter().all(|value| *value == 0) && &bytes[16..] == expected
}

fn take_array<const N: usize>(bytes: &Bytes, pos: &mut u32) -> Result<[u8; N], DecodeError> {
    let end = pos.checked_add(N as u32).ok_or(DecodeError::Truncated)?;
    if end > bytes.len() {
        return Err(DecodeError::Truncated);
    }

    let mut out = [0u8; N];
    bytes.slice(*pos..end).copy_into_slice(&mut out);
    *pos = end;
    Ok(out)
}

#[contract]
pub struct CollateralSufficiencyVerifier;

#[contractimpl]
impl CollateralSufficiencyVerifier {
    pub fn __constructor(env: Env, verification_key: Bytes) {
        let parsed = VerificationKey::from_bytes(&env, &verification_key);
        assert!(parsed.is_ok(), "invalid verification key");
        env.storage()
            .instance()
            .set(&DataKey::VerificationKey, &verification_key);
    }

    pub fn verify(env: Env, proof_type: u32, statement_hash: BytesN<32>, proof: Bytes) -> bool {
        if proof_type != PROOF_TYPE_COLLATERAL_SUFFICIENCY {
            return false;
        }

        let Some(vk_bytes) = env
            .storage()
            .instance()
            .get::<DataKey, Bytes>(&DataKey::VerificationKey)
        else {
            return false;
        };
        let Ok(vk) = VerificationKey::from_bytes(&env, &vk_bytes) else {
            return false;
        };
        let Ok(payload) = decode_proof_payload(&env, &proof) else {
            return false;
        };
        if !statement_hash_matches(&payload.public_signals, &statement_hash) {
            return false;
        }

        verify_groth16(&env, &vk, &payload.proof, &payload.public_signals)
    }
}

#[cfg(test)]
mod test;
