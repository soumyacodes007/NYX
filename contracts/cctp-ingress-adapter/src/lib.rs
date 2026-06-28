#![no_std]

use asset_registry::AssetRegistryClient;
use compliance_control::ComplianceControlClient;
use participant_registry::ParticipantRegistryClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env, String,
};
use zkdtcc_types::CctpMintReceipt;

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;

const CCTP_VERSION: u32 = 1;
const MESSAGE_HEADER_LEN: u32 = 148;
const BURN_BODY_MIN_LEN: u32 = 228;
const HOOK_RESERVED_LEN: u32 = 24;
const HOOK_FIXED_LEN: u32 = 32;
const HOOK_VERSION: u32 = 0;

const HEADER_SOURCE_DOMAIN_OFFSET: u32 = 4;
const HEADER_DESTINATION_DOMAIN_OFFSET: u32 = 8;
const HEADER_NONCE_OFFSET: u32 = 12;
const HEADER_DESTINATION_CALLER_OFFSET: u32 = 108;

const BODY_MINT_RECIPIENT_OFFSET: u32 = 36;
const BODY_AMOUNT_OFFSET: u32 = 68;
const BODY_HOOK_OFFSET: u32 = 228;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    ParticipantRegistry,
    AssetRegistry,
    ComplianceControl,
    UsdcAsset,
    ForwarderPayload,
    ExpectedDestinationDomain,
    ReceiptByNonce(u32, BytesN<32>),
    ReceiptBySession(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CctpIngressError {
    Unauthorized = 1,
    InvalidMessage = 2,
    InvalidVersion = 3,
    InvalidDestinationDomain = 4,
    ForwarderMismatch = 5,
    DuplicateNonce = 6,
    DuplicateSession = 7,
    UnsupportedAsset = 8,
    UnregisteredWallet = 9,
    InvalidAmount = 10,
    UnsupportedRecipientType = 11,
    ReceiptNotFound = 12,
    ProtocolPaused = 13,
    ParticipantFrozen = 14,
    AssetPaused = 15,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["cctp_receipt_recorded"])]
pub struct CctpReceiptRecordedEvent {
    pub source_domain: u32,
    pub nonce: BytesN<32>,
    pub forward_recipient: Address,
    pub amount_7_decimals: i128,
    pub session_id: BytesN<32>,
}

#[contract]
pub struct CctpIngressAdapter;

#[contractimpl]
impl CctpIngressAdapter {
    #[allow(clippy::too_many_arguments)]
    pub fn __constructor(
        env: Env,
        admin: Address,
        participant_registry: Address,
        asset_registry: Address,
        compliance_control: Address,
        usdc_asset: Address,
        forwarder_payload: BytesN<32>,
        expected_destination_domain: u32,
    ) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::ParticipantRegistry, &participant_registry);
        env.storage()
            .instance()
            .set(&DataKey::AssetRegistry, &asset_registry);
        env.storage()
            .instance()
            .set(&DataKey::ComplianceControl, &compliance_control);
        env.storage().instance().set(&DataKey::UsdcAsset, &usdc_asset);
        env.storage()
            .instance()
            .set(&DataKey::ForwarderPayload, &forwarder_payload);
        env.storage()
            .instance()
            .set(&DataKey::ExpectedDestinationDomain, &expected_destination_domain);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), CctpIngressError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn record_mint_receipt(
        env: Env,
        operator: Address,
        message: Bytes,
        attestation: Bytes,
        session_id: BytesN<32>,
    ) -> Result<CctpMintReceipt, CctpIngressError> {
        require_operator_auth(&env, &operator)?;

        let participant_registry: Address = env
            .storage()
            .instance()
            .get(&DataKey::ParticipantRegistry)
            .unwrap();
        let asset_registry: Address = env.storage().instance().get(&DataKey::AssetRegistry).unwrap();
        let compliance_control: Address = env
            .storage()
            .instance()
            .get(&DataKey::ComplianceControl)
            .unwrap();
        let usdc_asset: Address = env.storage().instance().get(&DataKey::UsdcAsset).unwrap();
        let forwarder_payload: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::ForwarderPayload)
            .unwrap();
        let expected_destination_domain: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ExpectedDestinationDomain)
            .unwrap();

        ensure_protocol_live(&env, &compliance_control, &usdc_asset)?;
        ensure_supported_asset(&env, &asset_registry, &usdc_asset)?;

        let parsed = parse_cctp_message(
            &env,
            &message,
            &forwarder_payload,
            expected_destination_domain,
        )?;

        ensure_wallet_registered(&env, &participant_registry, &parsed.forward_recipient)?;
        ensure_wallet_not_frozen(
            &env,
            &participant_registry,
            &compliance_control,
            &parsed.forward_recipient,
        )?;

        let nonce_key = DataKey::ReceiptByNonce(parsed.source_domain, parsed.nonce.clone());
        if env.storage().persistent().has(&nonce_key) {
            return Err(CctpIngressError::DuplicateNonce);
        }

        let session_key = DataKey::ReceiptBySession(session_id.clone());
        if env.storage().persistent().has(&session_key) {
            return Err(CctpIngressError::DuplicateSession);
        }

        let receipt = CctpMintReceipt {
            source_domain: parsed.source_domain,
            destination_domain: parsed.destination_domain,
            nonce: parsed.nonce.clone(),
            forward_recipient: parsed.forward_recipient.clone(),
            usdc_asset,
            amount_6_decimals: parsed.amount_6_decimals,
            amount_7_decimals: parsed.amount_7_decimals,
            session_id: session_id.clone(),
            message_hash: env.crypto().sha256(&message).into(),
            attestation_hash: env.crypto().sha256(&attestation).into(),
            recorded_ledger: env.ledger().sequence(),
        };

        env.storage().persistent().set(&nonce_key, &receipt);
        env.storage().persistent().set(&session_key, &parsed.nonce);
        bump_persistent(&env, &nonce_key);
        bump_persistent(&env, &session_key);
        bump_instance(&env);

        CctpReceiptRecordedEvent {
            source_domain: receipt.source_domain,
            nonce: receipt.nonce.clone(),
            forward_recipient: receipt.forward_recipient.clone(),
            amount_7_decimals: receipt.amount_7_decimals,
            session_id: receipt.session_id.clone(),
        }
        .publish(&env);

        Ok(receipt)
    }

    pub fn get_receipt_by_nonce(
        env: Env,
        source_domain: u32,
        nonce: BytesN<32>,
    ) -> Result<CctpMintReceipt, CctpIngressError> {
        let key = DataKey::ReceiptByNonce(source_domain, nonce);
        let receipt = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(CctpIngressError::ReceiptNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(receipt)
    }

    pub fn get_receipt_by_session(
        env: Env,
        session_id: BytesN<32>,
        source_domain: u32,
    ) -> Result<CctpMintReceipt, CctpIngressError> {
        let session_key = DataKey::ReceiptBySession(session_id);
        let nonce: BytesN<32> = env
            .storage()
            .persistent()
            .get(&session_key)
            .ok_or(CctpIngressError::ReceiptNotFound)?;
        bump_persistent(&env, &session_key);
        bump_instance(&env);
        Self::get_receipt_by_nonce(env, source_domain, nonce)
    }

    pub fn normalize_amount_6_to_7(
        _env: Env,
        amount_6_decimals: i128,
    ) -> Result<i128, CctpIngressError> {
        normalize_amount(amount_6_decimals)
    }
}

#[derive(Clone)]
struct ParsedMessage {
    source_domain: u32,
    destination_domain: u32,
    nonce: BytesN<32>,
    forward_recipient: Address,
    amount_6_decimals: i128,
    amount_7_decimals: i128,
}

fn parse_cctp_message(
    env: &Env,
    message: &Bytes,
    forwarder_payload: &BytesN<32>,
    expected_destination_domain: u32,
) -> Result<ParsedMessage, CctpIngressError> {
    if message.len() < MESSAGE_HEADER_LEN + BURN_BODY_MIN_LEN + HOOK_FIXED_LEN {
        return Err(CctpIngressError::InvalidMessage);
    }

    let version = read_u32_be(message, 0)?;
    if version != CCTP_VERSION {
        return Err(CctpIngressError::InvalidVersion);
    }

    let source_domain = read_u32_be(message, HEADER_SOURCE_DOMAIN_OFFSET)?;
    let destination_domain = read_u32_be(message, HEADER_DESTINATION_DOMAIN_OFFSET)?;
    if destination_domain != expected_destination_domain {
        return Err(CctpIngressError::InvalidDestinationDomain);
    }

    let nonce = read_bytes32(message, HEADER_NONCE_OFFSET)?;
    let destination_caller = read_bytes32(message, HEADER_DESTINATION_CALLER_OFFSET)?;
    let mint_recipient = read_bytes32(message, MESSAGE_HEADER_LEN + BODY_MINT_RECIPIENT_OFFSET)?;
    if destination_caller != *forwarder_payload || mint_recipient != *forwarder_payload {
        return Err(CctpIngressError::ForwarderMismatch);
    }

    let amount_6_decimals =
        read_u256_as_i128(message, MESSAGE_HEADER_LEN + BODY_AMOUNT_OFFSET)?;
    let amount_7_decimals = normalize_amount(amount_6_decimals)?;

    let hook = message.slice((MESSAGE_HEADER_LEN + BODY_HOOK_OFFSET)..);
    let forward_recipient = parse_forward_recipient(env, &hook)?;

    Ok(ParsedMessage {
        source_domain,
        destination_domain,
        nonce,
        forward_recipient,
        amount_6_decimals,
        amount_7_decimals,
    })
}

fn parse_forward_recipient(_env: &Env, hook: &Bytes) -> Result<Address, CctpIngressError> {
    if hook.len() < HOOK_FIXED_LEN {
        return Err(CctpIngressError::InvalidMessage);
    }

    for i in 0..HOOK_RESERVED_LEN {
        if hook.get_unchecked(i) != 0 {
            return Err(CctpIngressError::InvalidMessage);
        }
    }

    let hook_version = read_u32_be(hook, HOOK_RESERVED_LEN)?;
    if hook_version != HOOK_VERSION {
        return Err(CctpIngressError::InvalidVersion);
    }

    let recipient_len = read_u32_be(hook, HOOK_RESERVED_LEN + 4)?;
    if recipient_len == 0 || hook.len() < HOOK_FIXED_LEN + recipient_len {
        return Err(CctpIngressError::InvalidMessage);
    }

    let recipient_bytes = hook.slice(HOOK_FIXED_LEN..(HOOK_FIXED_LEN + recipient_len));
    match recipient_bytes.first() {
        Some(b'M') => return Err(CctpIngressError::UnsupportedRecipientType),
        Some(b'G') | Some(b'C') => {}
        _ => return Err(CctpIngressError::InvalidMessage),
    }

    let recipient_string = String::from(recipient_bytes);
    Ok(Address::from_string(&recipient_string))
}

fn read_u32_be(bytes: &Bytes, offset: u32) -> Result<u32, CctpIngressError> {
    let slice = bytes.slice(offset..(offset + 4));
    let mut raw = [0u8; 4];
    slice.copy_into_slice(&mut raw);
    Ok(u32::from_be_bytes(raw))
}

fn read_bytes32(bytes: &Bytes, offset: u32) -> Result<BytesN<32>, CctpIngressError> {
    let slice = bytes.slice(offset..(offset + 32));
    let mut raw = [0u8; 32];
    slice.copy_into_slice(&mut raw);
    Ok(BytesN::from_array(bytes.env(), &raw))
}

fn read_u256_as_i128(bytes: &Bytes, offset: u32) -> Result<i128, CctpIngressError> {
    let slice = bytes.slice(offset..(offset + 32));
    let mut raw = [0u8; 32];
    slice.copy_into_slice(&mut raw);

    if raw[..16].iter().any(|byte| *byte != 0) {
        return Err(CctpIngressError::InvalidAmount);
    }

    let mut low = [0u8; 16];
    low.copy_from_slice(&raw[16..]);
    let amount = i128::from_be_bytes(low);
    if amount <= 0 {
        return Err(CctpIngressError::InvalidAmount);
    }
    Ok(amount)
}

fn normalize_amount(amount_6_decimals: i128) -> Result<i128, CctpIngressError> {
    amount_6_decimals
        .checked_mul(10)
        .ok_or(CctpIngressError::InvalidAmount)
}

fn ensure_supported_asset(
    env: &Env,
    asset_registry: &Address,
    usdc_asset: &Address,
) -> Result<(), CctpIngressError> {
    let client = AssetRegistryClient::new(env, asset_registry);
    if !client.is_supported_asset(usdc_asset) {
        return Err(CctpIngressError::UnsupportedAsset);
    }
    Ok(())
}

fn ensure_wallet_registered(
    env: &Env,
    participant_registry: &Address,
    wallet: &Address,
) -> Result<(), CctpIngressError> {
    let client = ParticipantRegistryClient::new(env, participant_registry);
    if !client.is_wallet_registered(wallet) {
        return Err(CctpIngressError::UnregisteredWallet);
    }
    Ok(())
}

fn ensure_protocol_live(
    env: &Env,
    compliance_control: &Address,
    usdc_asset: &Address,
) -> Result<(), CctpIngressError> {
    let compliance = ComplianceControlClient::new(env, compliance_control);
    if compliance.is_globally_paused() {
        return Err(CctpIngressError::ProtocolPaused);
    }
    if compliance.is_asset_paused(usdc_asset) {
        return Err(CctpIngressError::AssetPaused);
    }
    Ok(())
}

fn ensure_wallet_not_frozen(
    env: &Env,
    participant_registry: &Address,
    compliance_control: &Address,
    wallet: &Address,
) -> Result<(), CctpIngressError> {
    let registry = ParticipantRegistryClient::new(env, participant_registry);
    let participant_id_hash = registry.wallet_owner(wallet);
    let compliance = ComplianceControlClient::new(env, compliance_control);
    if compliance.is_participant_frozen(&participant_id_hash) {
        return Err(CctpIngressError::ParticipantFrozen);
    }
    Ok(())
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), CctpIngressError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(CctpIngressError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), CctpIngressError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(CctpIngressError::Unauthorized);
    }
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(())
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_TO);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_TO);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use asset_registry::AssetRegistryArgs;
    use compliance_control::ComplianceControlArgs;
    use participant_registry::ParticipantRegistryArgs;
    use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, IntoVal, Symbol};
    use zkdtcc_types::AssetClass;

    const TEST_DESTINATION_DOMAIN: u32 = 27;

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn build_message(
        env: &Env,
        source_domain: u32,
        destination_domain: u32,
        nonce: [u8; 32],
        forwarder_payload: [u8; 32],
        amount_6_decimals: u128,
        forward_recipient: &Address,
        hook_version: u32,
    ) -> Bytes {
        let forward_strkey = forward_recipient.to_string();
        let mut recipient_bytes = std::vec![0u8; forward_strkey.len() as usize];
        forward_strkey.copy_into_slice(&mut recipient_bytes);

        let mut message =
            std::vec![0u8; (MESSAGE_HEADER_LEN + BODY_HOOK_OFFSET + HOOK_FIXED_LEN) as usize];
        message[0..4].copy_from_slice(&CCTP_VERSION.to_be_bytes());
        message[HEADER_SOURCE_DOMAIN_OFFSET as usize..(HEADER_SOURCE_DOMAIN_OFFSET + 4) as usize]
            .copy_from_slice(&source_domain.to_be_bytes());
        message[HEADER_DESTINATION_DOMAIN_OFFSET as usize
            ..(HEADER_DESTINATION_DOMAIN_OFFSET + 4) as usize]
            .copy_from_slice(&destination_domain.to_be_bytes());
        message[HEADER_NONCE_OFFSET as usize..(HEADER_NONCE_OFFSET + 32) as usize]
            .copy_from_slice(&nonce);
        message[HEADER_DESTINATION_CALLER_OFFSET as usize
            ..(HEADER_DESTINATION_CALLER_OFFSET + 32) as usize]
            .copy_from_slice(&forwarder_payload);
        message[(MESSAGE_HEADER_LEN + BODY_MINT_RECIPIENT_OFFSET) as usize
            ..(MESSAGE_HEADER_LEN + BODY_MINT_RECIPIENT_OFFSET + 32) as usize]
            .copy_from_slice(&forwarder_payload);

        let mut amount = [0u8; 32];
        amount[16..].copy_from_slice(&amount_6_decimals.to_be_bytes());
        message[(MESSAGE_HEADER_LEN + BODY_AMOUNT_OFFSET) as usize
            ..(MESSAGE_HEADER_LEN + BODY_AMOUNT_OFFSET + 32) as usize]
            .copy_from_slice(&amount);

        let hook_offset = (MESSAGE_HEADER_LEN + BODY_HOOK_OFFSET) as usize;
        message[hook_offset + 24..hook_offset + 28].copy_from_slice(&hook_version.to_be_bytes());
        message[hook_offset + 28..hook_offset + 32]
            .copy_from_slice(&(recipient_bytes.len() as u32).to_be_bytes());
        message.extend_from_slice(&recipient_bytes);

        Bytes::from_slice(env, &message)
    }

    fn setup_phase_zero(
        env: &Env,
    ) -> (
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        Address,
        [u8; 32],
    ) {
        env.mock_all_auths();

        let admin = Address::generate(env);
        let operator = Address::generate(env);
        let wallet = Address::generate(env);
        let asset_registry_id = env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
        let participant_registry_id = env.register(
            participant_registry::ParticipantRegistry,
            ParticipantRegistryArgs::__constructor(&admin),
        );
        let asset_registry = asset_registry::AssetRegistryClient::new(env, &asset_registry_id);
        let participant_registry =
            participant_registry::ParticipantRegistryClient::new(env, &participant_registry_id);
        let compliance_control_id = env.register(
            compliance_control::ComplianceControl,
            ComplianceControlArgs::__constructor(&admin),
        );
        let compliance_control =
            compliance_control::ComplianceControlClient::new(env, &compliance_control_id);

        asset_registry.set_operator(&admin, &operator, &true);
        participant_registry.set_operator(&admin, &operator, &true);
        compliance_control.set_operator(&admin, &operator, &true);

        let usdc_asset = Address::generate(env);
        let issuer = Address::generate(env);
        asset_registry.register_asset(
            &operator,
            &usdc_asset,
            &hash(env, 10),
            &issuer,
            &AssetClass::UsdcSac,
            &true,
            &true,
            &true,
            &true,
            &hash(env, 11),
            &hash(env, 12),
        );

        participant_registry.register_participant(
            &operator,
            &hash(env, 20),
            &wallet,
            &zkdtcc_types::ParticipantRole::InstitutionTrader,
            &hash(env, 21),
            &hash(env, 22),
            &hash(env, 23),
        );

        let mut payload = [0u8; 32];
        payload.copy_from_slice(&hash(env, 99).to_array());

        (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            payload,
        )
    }

    #[test]
    fn records_cctp_receipt_from_message() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let message = build_message(
            &env,
            0,
            TEST_DESTINATION_DOMAIN,
            [7; 32],
            forwarder_payload,
            1_250_000u128,
            &wallet,
            HOOK_VERSION,
        );
        let attestation = Bytes::from_slice(&env, &[1, 2, 3, 4]);
        let receipt = client.record_mint_receipt(&operator, &message, &attestation, &hash(&env, 42));

        assert_eq!(receipt.source_domain, 0);
        assert_eq!(receipt.destination_domain, TEST_DESTINATION_DOMAIN);
        assert_eq!(receipt.forward_recipient, wallet);
        assert_eq!(receipt.amount_6_decimals, 1_250_000);
        assert_eq!(receipt.amount_7_decimals, 12_500_000);
        assert_eq!(
            client.get_receipt_by_nonce(&0, &BytesN::from_array(&env, &[7; 32])),
            receipt
        );
    }

    #[test]
    fn rejects_duplicate_nonce() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let message = build_message(
            &env,
            0,
            TEST_DESTINATION_DOMAIN,
            [8; 32],
            forwarder_payload,
            10u128,
            &wallet,
            HOOK_VERSION,
        );
        let attestation = Bytes::from_slice(&env, &[9]);
        client.record_mint_receipt(&operator, &message, &attestation, &hash(&env, 1));

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                message.into_val(&env),
                attestation.into_val(&env),
                hash(&env, 2).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::DuplicateNonce))));
    }

    #[test]
    fn rejects_duplicate_session() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let session_id = hash(&env, 50);
        let attestation = Bytes::from_slice(&env, &[1]);
        client.record_mint_receipt(
            &operator,
            &build_message(&env, 0, TEST_DESTINATION_DOMAIN, [1; 32], forwarder_payload, 10, &wallet, HOOK_VERSION),
            &attestation,
            &session_id,
        );

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, TEST_DESTINATION_DOMAIN, [2; 32], forwarder_payload, 11, &wallet, HOOK_VERSION).into_val(&env),
                attestation.into_val(&env),
                session_id.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::DuplicateSession))));
    }

    #[test]
    fn rejects_wrong_destination_domain() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, 99, [1; 32], forwarder_payload, 10, &wallet, HOOK_VERSION).into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
                hash(&env, 60).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::InvalidDestinationDomain))));
    }

    #[test]
    fn rejects_forwarder_payload_mismatch() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, TEST_DESTINATION_DOMAIN, [1; 32], [6; 32], 10, &wallet, HOOK_VERSION).into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
                hash(&env, 70).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::ForwarderMismatch))));
    }

    #[test]
    fn rejects_unregistered_wallet() {
        let env = Env::default();
        let (
            admin,
            operator,
            _wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);
        let unknown_wallet = Address::generate(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, TEST_DESTINATION_DOMAIN, [1; 32], forwarder_payload, 10, &unknown_wallet, HOOK_VERSION).into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
                hash(&env, 71).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::UnregisteredWallet))));
    }

    #[test]
    fn rejects_unsupported_asset() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let asset_registry = asset_registry::AssetRegistryClient::new(&env, &asset_registry_id);
        asset_registry.set_status(&operator, &usdc_asset, &zkdtcc_types::AssetStatus::Suspended);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, TEST_DESTINATION_DOMAIN, [1; 32], forwarder_payload, 10, &wallet, HOOK_VERSION).into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
                hash(&env, 72).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::UnsupportedAsset))));
    }

    #[test]
    fn rejects_invalid_hook_version() {
        let env = Env::default();
        let (
            admin,
            operator,
            wallet,
            asset_registry_id,
            participant_registry_id,
            compliance_control_id,
            usdc_asset,
            forwarder_payload,
        ) = setup_phase_zero(&env);

        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &admin,
                &participant_registry_id,
                &asset_registry_id,
                &compliance_control_id,
                &usdc_asset,
                &BytesN::from_array(&env, &forwarder_payload),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<CctpMintReceipt, CctpIngressError>(
            &contract_id,
            &Symbol::new(&env, "record_mint_receipt"),
            vec![
                &env,
                operator.into_val(&env),
                build_message(&env, 0, TEST_DESTINATION_DOMAIN, [1; 32], forwarder_payload, 10, &wallet, 1).into_val(&env),
                Bytes::from_slice(&env, &[1]).into_val(&env),
                hash(&env, 73).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CctpIngressError::InvalidVersion))));
    }

    #[test]
    fn normalizes_amounts_exactly() {
        let env = Env::default();
        let contract_id = env.register(
            CctpIngressAdapter,
            CctpIngressAdapterArgs::__constructor(
                &Address::generate(&env),
                &Address::generate(&env),
                &Address::generate(&env),
                &Address::generate(&env),
                &Address::generate(&env),
                &hash(&env, 1),
                &TEST_DESTINATION_DOMAIN,
            ),
        );
        let client = CctpIngressAdapterClient::new(&env, &contract_id);
        assert_eq!(client.normalize_amount_6_to_7(&1_234_567), 12_345_670);
    }
}
