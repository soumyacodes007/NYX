#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
};
use zkdtcc_types::{
    KycStatus, ParticipantRecord, ParticipantRole, ParticipantStatus, SanctionsStatus,
};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    Participant(BytesN<32>),
    Wallet(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ParticipantRegistryError {
    Unauthorized = 1,
    ParticipantExists = 2,
    ParticipantNotFound = 3,
    WalletExists = 4,
    WalletNotFound = 5,
    PrimaryWalletRequired = 6,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["participant_registered"])]
pub struct ParticipantRegisteredEvent {
    pub participant_id_hash: BytesN<32>,
    pub primary_wallet: Address,
}

#[contractevent(topics = ["wallet_added"])]
pub struct WalletAddedEvent {
    pub participant_id_hash: BytesN<32>,
    pub wallet: Address,
}

#[contract]
pub struct ParticipantRegistry;

#[contractimpl]
impl ParticipantRegistry {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), ParticipantRegistryError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_participant(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        primary_wallet: Address,
        role: ParticipantRole,
        credential_root: BytesN<32>,
        legal_entity_hash: BytesN<32>,
        jurisdiction_hash: BytesN<32>,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;

        let participant_key = DataKey::Participant(participant_id_hash.clone());
        if env.storage().persistent().has(&participant_key) {
            return Err(ParticipantRegistryError::ParticipantExists);
        }

        let wallet_key = DataKey::Wallet(primary_wallet.clone());
        if env.storage().persistent().has(&wallet_key) {
            return Err(ParticipantRegistryError::WalletExists);
        }

        let ledger = env.ledger().sequence();
        let record = ParticipantRecord {
            primary_wallet: primary_wallet.clone(),
            role,
            status: ParticipantStatus::Active,
            credential_root,
            legal_entity_hash,
            jurisdiction_hash,
            kyc_status: KycStatus::Approved,
            sanctions_status: SanctionsStatus::Clear,
            credential_expiry_ledger: u32::MAX,
            review_case_id: zero_hash(&env),
            permissions_hash: zero_hash(&env),
            wallet_count: 1,
            created_ledger: ledger,
            updated_ledger: ledger,
        };

        env.storage().persistent().set(&participant_key, &record);
        env.storage()
            .persistent()
            .set(&wallet_key, &participant_id_hash);

        bump_persistent(&env, &participant_key);
        bump_persistent(&env, &wallet_key);
        bump_instance(&env);
        ParticipantRegisteredEvent {
            participant_id_hash,
            primary_wallet,
        }
        .publish(&env);
        Ok(())
    }

    pub fn add_wallet(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        wallet: Address,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let wallet_key = DataKey::Wallet(wallet.clone());
        if env.storage().persistent().has(&wallet_key) {
            return Err(ParticipantRegistryError::WalletExists);
        }

        let mut record = load_participant(&env, &participant_id_hash)?;
        record.wallet_count = record.wallet_count.checked_add(1).unwrap();
        record.updated_ledger = env.ledger().sequence();

        save_participant(&env, &participant_id_hash, &record);
        env.storage()
            .persistent()
            .set(&wallet_key, &participant_id_hash);
        bump_persistent(&env, &wallet_key);
        WalletAddedEvent {
            participant_id_hash,
            wallet,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_primary_wallet(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        wallet: Address,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let wallet_owner = load_wallet_owner(&env, &wallet)?;
        if wallet_owner != participant_id_hash {
            return Err(ParticipantRegistryError::WalletNotFound);
        }

        let mut record = load_participant(&env, &participant_id_hash)?;
        record.primary_wallet = wallet;
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        Ok(())
    }

    pub fn remove_wallet(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        wallet: Address,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_participant(&env, &participant_id_hash)?;
        if record.primary_wallet == wallet {
            return Err(ParticipantRegistryError::PrimaryWalletRequired);
        }

        let wallet_owner = load_wallet_owner(&env, &wallet)?;
        if wallet_owner != participant_id_hash {
            return Err(ParticipantRegistryError::WalletNotFound);
        }

        let wallet_key = DataKey::Wallet(wallet);
        env.storage().persistent().remove(&wallet_key);
        record.wallet_count = record.wallet_count.checked_sub(1).unwrap();
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        bump_instance(&env);
        Ok(())
    }

    pub fn set_status(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        status: ParticipantStatus,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_participant(&env, &participant_id_hash)?;
        record.status = status;
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        Ok(())
    }

    pub fn update_credential_root(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        credential_root: BytesN<32>,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_participant(&env, &participant_id_hash)?;
        record.credential_root = credential_root;
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        Ok(())
    }

    pub fn set_compliance_state(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        kyc_status: KycStatus,
        sanctions_status: SanctionsStatus,
        credential_expiry_ledger: u32,
        review_case_id: BytesN<32>,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_participant(&env, &participant_id_hash)?;
        record.kyc_status = kyc_status;
        record.sanctions_status = sanctions_status;
        record.credential_expiry_ledger = credential_expiry_ledger;
        record.review_case_id = review_case_id;
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        Ok(())
    }

    pub fn set_permissions_hash(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        permissions_hash: BytesN<32>,
    ) -> Result<(), ParticipantRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_participant(&env, &participant_id_hash)?;
        record.permissions_hash = permissions_hash;
        record.updated_ledger = env.ledger().sequence();
        save_participant(&env, &participant_id_hash, &record);
        Ok(())
    }

    pub fn is_participant_trade_eligible(
        env: Env,
        participant_id_hash: BytesN<32>,
        _asset: Address,
    ) -> bool {
        match load_participant(&env, &participant_id_hash) {
            Ok(record) => {
                record.status == ParticipantStatus::Active
                    && record.kyc_status == KycStatus::Approved
                    && record.sanctions_status == SanctionsStatus::Clear
                    && env.ledger().sequence() <= record.credential_expiry_ledger
            }
            Err(_) => false,
        }
    }

    pub fn get_participant(
        env: Env,
        participant_id_hash: BytesN<32>,
    ) -> Result<ParticipantRecord, ParticipantRegistryError> {
        load_participant(&env, &participant_id_hash)
    }

    pub fn wallet_owner(
        env: Env,
        wallet: Address,
    ) -> Result<BytesN<32>, ParticipantRegistryError> {
        load_wallet_owner(&env, &wallet)
    }

    pub fn is_wallet_registered(env: Env, wallet: Address) -> bool {
        load_wallet_owner(&env, &wallet).is_ok()
    }
}

fn load_participant(
    env: &Env,
    participant_id_hash: &BytesN<32>,
) -> Result<ParticipantRecord, ParticipantRegistryError> {
    let key = DataKey::Participant(participant_id_hash.clone());
    let record = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ParticipantRegistryError::ParticipantNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(record)
}

fn save_participant(env: &Env, participant_id_hash: &BytesN<32>, record: &ParticipantRecord) {
    let key = DataKey::Participant(participant_id_hash.clone());
    env.storage().persistent().set(&key, record);
    bump_persistent(env, &key);
    bump_instance(env);
}

fn load_wallet_owner(
    env: &Env,
    wallet: &Address,
) -> Result<BytesN<32>, ParticipantRegistryError> {
    let key = DataKey::Wallet(wallet.clone());
    let owner = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(ParticipantRegistryError::WalletNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(owner)
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), ParticipantRegistryError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(ParticipantRegistryError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), ParticipantRegistryError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(ParticipantRegistryError::Unauthorized);
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

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, IntoVal, Symbol};

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    #[test]
    fn registers_and_manages_wallets() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let wallet_one = Address::generate(&env);
        let wallet_two = Address::generate(&env);
        let participant_id = hash(&env, 1);

        let contract_id =
            env.register(ParticipantRegistry, ParticipantRegistryArgs::__constructor(&admin));
        let client = ParticipantRegistryClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.register_participant(
            &operator,
            &participant_id,
            &wallet_one,
            &ParticipantRole::InstitutionTrader,
            &hash(&env, 2),
            &hash(&env, 3),
            &hash(&env, 4),
        );

        client.add_wallet(&operator, &participant_id, &wallet_two);
        client.set_primary_wallet(&operator, &participant_id, &wallet_two);

        let record = client.get_participant(&participant_id);
        assert_eq!(record.primary_wallet, wallet_two);
        assert_eq!(record.wallet_count, 2);
        assert_eq!(record.kyc_status, KycStatus::Approved);
        assert_eq!(record.sanctions_status, SanctionsStatus::Clear);
        assert_eq!(client.wallet_owner(&wallet_one), participant_id);
        assert!(client.is_wallet_registered(&wallet_two));

        client.remove_wallet(&operator, &participant_id, &wallet_one);
        assert_eq!(client.get_participant(&participant_id).wallet_count, 1);
    }

    #[test]
    fn rejects_duplicate_wallets() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let wallet = Address::generate(&env);
        let contract_id =
            env.register(ParticipantRegistry, ParticipantRegistryArgs::__constructor(&admin));
        let client = ParticipantRegistryClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.register_participant(
            &operator,
            &hash(&env, 10),
            &wallet,
            &ParticipantRole::InstitutionTrader,
            &hash(&env, 11),
            &hash(&env, 12),
            &hash(&env, 13),
        );

        let result = env.try_invoke_contract::<(), ParticipantRegistryError>(
            &contract_id,
            &Symbol::new(&env, "register_participant"),
            vec![
                &env,
                operator.into_val(&env),
                hash(&env, 14).into_val(&env),
                wallet.into_val(&env),
                ParticipantRole::Auditor.into_val(&env),
                hash(&env, 15).into_val(&env),
                hash(&env, 16).into_val(&env),
                hash(&env, 17).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(ParticipantRegistryError::WalletExists))));
    }

    #[test]
    fn updates_compliance_state_and_trade_eligibility() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let wallet = Address::generate(&env);
        let asset = Address::generate(&env);
        let participant_id = hash(&env, 21);

        let contract_id =
            env.register(ParticipantRegistry, ParticipantRegistryArgs::__constructor(&admin));
        let client = ParticipantRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.register_participant(
            &operator,
            &participant_id,
            &wallet,
            &ParticipantRole::InstitutionTrader,
            &hash(&env, 22),
            &hash(&env, 23),
            &hash(&env, 24),
        );

        assert!(client.is_participant_trade_eligible(&participant_id, &asset));
        client.set_compliance_state(
            &operator,
            &participant_id,
            &KycStatus::Approved,
            &SanctionsStatus::Blocked,
            &u32::MAX,
            &hash(&env, 25),
        );
        client.set_permissions_hash(&operator, &participant_id, &hash(&env, 26));

        let record = client.get_participant(&participant_id);
        assert_eq!(record.permissions_hash, hash(&env, 26));
        assert_eq!(record.review_case_id, hash(&env, 25));
        assert!(!client.is_participant_trade_eligible(&participant_id, &asset));
    }
}
