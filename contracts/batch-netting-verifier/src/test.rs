extern crate std;

use super::*;
use serde_json::Value;
use soroban_sdk::{Bytes, BytesN, Env, U256, Vec};
use std::{path::PathBuf, process::Command};

fn fr_from_u128(env: &Env, value: u128) -> Fr {
    Fr::from_u256(U256::from_u128(env, value))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn decode_hex(hex: &str) -> std::vec::Vec<u8> {
    let raw = hex.strip_prefix("0x").unwrap_or(hex);
    let mut out = std::vec::Vec::with_capacity(raw.len() / 2);
    let bytes = raw.as_bytes();
    for index in (0..bytes.len()).step_by(2) {
        let hi = (bytes[index] as char).to_digit(16).unwrap() as u8;
        let lo = (bytes[index + 1] as char).to_digit(16).unwrap() as u8;
        out.push((hi << 4) | lo);
    }
    out
}

#[test]
fn payload_roundtrip_preserves_statement_hash_binding() {
    let env = Env::default();
    let statement_hash = BytesN::from_array(
        &env,
        &[
            0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d,
            0x2e, 0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a,
            0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x40,
        ],
    );

    let proof = Proof {
        a: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0; BN254_G1_SERIALIZED_SIZE])),
        b: Bn254G2Affine::from_bytes(BytesN::from_array(&env, &[0; BN254_G2_SERIALIZED_SIZE])),
        c: Bn254G1Affine::from_bytes(BytesN::from_array(&env, &[0; BN254_G1_SERIALIZED_SIZE])),
    };

    let mut public_signals = Vec::new(&env);
    public_signals.push_back(fr_from_u128(&env, 1));
    public_signals.push_back(fr_from_u128(&env, 2));
    public_signals.push_back(fr_from_u128(&env, 3));
    public_signals.push_back(fr_from_u128(&env, 4));
    public_signals.push_back(fr_from_u128(&env, 5));

    let mut hi_bytes = [0u8; 32];
    hi_bytes[16..].copy_from_slice(&statement_hash.to_array()[..16]);
    public_signals.push_back(Fr::from_bytes(BytesN::from_array(&env, &hi_bytes)));

    let mut lo_bytes = [0u8; 32];
    lo_bytes[16..].copy_from_slice(&statement_hash.to_array()[16..]);
    public_signals.push_back(Fr::from_bytes(BytesN::from_array(&env, &lo_bytes)));

    let payload = encode_proof_payload(&env, &proof, &public_signals);
    let decoded = decode_proof_payload(&env, &payload).unwrap();

    assert_eq!(decoded.public_signals.len(), public_signals.len());
    assert!(statement_hash_matches(&decoded.public_signals, &statement_hash));
}

#[test]
fn verifies_real_generated_bundle() {
    let output = Command::new("node")
        .current_dir(repo_root())
        .arg("scripts/generate-batch-netting-proof.mjs")
        .arg("0x8899aabbccddeeff00112233445566778899aabbccddeeff0011223344556677")
        .arg("verifier-debug")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bundle generation failed\nstdout:\n{}\nstderr:\n{}",
        std::string::String::from_utf8_lossy(&output.stdout),
        std::string::String::from_utf8_lossy(&output.stderr),
    );

    let stdout = std::string::String::from_utf8(output.stdout).unwrap();
    let marker = "__PHASE5_BUNDLE__";
    let line = stdout
        .lines()
        .find(|line| line.starts_with(marker))
        .expect("missing proof bundle marker");
    let summary: Value = serde_json::from_str(&line[marker.len()..]).unwrap();
    let bundle = &summary["bundle"];

    let env = Env::default();
    let statement_hash = BytesN::from_array(
        &env,
        &decode_hex(bundle["statementHashHex"].as_str().unwrap())
            .try_into()
            .unwrap(),
    );
    let payload = Bytes::from_slice(&env, &decode_hex(bundle["proofPayloadHex"].as_str().unwrap()));
    let vk_bytes = Bytes::from_slice(
        &env,
        &decode_hex(bundle["verificationKeyHex"].as_str().unwrap()),
    );

    let decoded = decode_proof_payload(&env, &payload).unwrap();
    let vk = VerificationKey::from_bytes(&env, &vk_bytes).unwrap();

    assert!(statement_hash_matches(&decoded.public_signals, &statement_hash));
    assert!(verify_groth16(
        &env,
        &vk,
        &decoded.proof,
        &decoded.public_signals
    ));
}
