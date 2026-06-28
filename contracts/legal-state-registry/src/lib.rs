#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
};
use zkdtcc_types::{LegalStateRecord, LegalStateStatus};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    State(BytesN<32>),
    CurrentEntitlement(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum LegalStateRegistryError {
    Unauthorized = 1,
    StateExists = 2,
    StateNotFound = 3,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["legal_state_recorded"])]
pub struct LegalStateRecordedEvent {
    pub entitlement_id_hash: BytesN<32>,
    pub state_id_hash: BytesN<32>,
}

#[contract]
pub struct LegalStateRegistry;

#[contractimpl]
impl LegalStateRegistry {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), LegalStateRegistryError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn record_state(
        env: Env,
        operator: Address,
        state_id_hash: BytesN<32>,
        participant_id_hash: BytesN<32>,
        wallet: Address,
        entitlement_id_hash: BytesN<32>,
        asset: Address,
        event_date: u64,
        issuer_policy_hash: BytesN<32>,
        state_commitment: BytesN<32>,
    ) -> Result<(), LegalStateRegistryError> {
        require_operator_auth(&env, &operator)?;

        let state_key = DataKey::State(state_id_hash.clone());
        if env.storage().persistent().has(&state_key) {
            return Err(LegalStateRegistryError::StateExists);
        }

        supersede_current_state(&env, &entitlement_id_hash)?;

        let ledger = env.ledger().sequence();
        let record = LegalStateRecord {
            participant_id_hash,
            wallet,
            entitlement_id_hash: entitlement_id_hash.clone(),
            asset,
            event_date,
            issuer_policy_hash,
            state_commitment,
            status: LegalStateStatus::Active,
            created_ledger: ledger,
            updated_ledger: ledger,
        };

        env.storage().persistent().set(&state_key, &record);
        bump_persistent(&env, &state_key);

        let entitlement_key = DataKey::CurrentEntitlement(entitlement_id_hash.clone());
        env.storage()
            .persistent()
            .set(&entitlement_key, &state_id_hash);
        bump_persistent(&env, &entitlement_key);
        bump_instance(&env);

        LegalStateRecordedEvent {
            entitlement_id_hash,
            state_id_hash,
        }
        .publish(&env);
        Ok(())
    }

    pub fn archive_state(
        env: Env,
        operator: Address,
        state_id_hash: BytesN<32>,
    ) -> Result<(), LegalStateRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_state(&env, &state_id_hash)?;
        record.status = LegalStateStatus::Archived;
        record.updated_ledger = env.ledger().sequence();
        save_state(&env, &state_id_hash, &record);
        Ok(())
    }

    pub fn get_state(
        env: Env,
        state_id_hash: BytesN<32>,
    ) -> Result<LegalStateRecord, LegalStateRegistryError> {
        load_state(&env, &state_id_hash)
    }

    pub fn current_state_for_entitlement(
        env: Env,
        entitlement_id_hash: BytesN<32>,
    ) -> Result<BytesN<32>, LegalStateRegistryError> {
        let key = DataKey::CurrentEntitlement(entitlement_id_hash);
        let state_id = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(LegalStateRegistryError::StateNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(state_id)
    }
}

fn supersede_current_state(
    env: &Env,
    entitlement_id_hash: &BytesN<32>,
) -> Result<(), LegalStateRegistryError> {
    let entitlement_key = DataKey::CurrentEntitlement(entitlement_id_hash.clone());
    let current_state_id: Option<BytesN<32>> = env.storage().persistent().get(&entitlement_key);
    if let Some(state_id) = current_state_id {
        let mut record = load_state(env, &state_id)?;
        record.status = LegalStateStatus::Superseded;
        record.updated_ledger = env.ledger().sequence();
        save_state(env, &state_id, &record);
    }
    Ok(())
}

fn load_state(
    env: &Env,
    state_id_hash: &BytesN<32>,
) -> Result<LegalStateRecord, LegalStateRegistryError> {
    let key = DataKey::State(state_id_hash.clone());
    let record = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(LegalStateRegistryError::StateNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(record)
}

fn save_state(env: &Env, state_id_hash: &BytesN<32>, record: &LegalStateRecord) {
    let key = DataKey::State(state_id_hash.clone());
    env.storage().persistent().set(&key, record);
    bump_persistent(env, &key);
    bump_instance(env);
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), LegalStateRegistryError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(LegalStateRegistryError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), LegalStateRegistryError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(LegalStateRegistryError::Unauthorized);
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
    use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, IntoVal, Symbol};

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    #[test]
    fn records_and_supersedes_entitlement_state() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let wallet = Address::generate(&env);
        let asset = Address::generate(&env);

        let contract_id =
            env.register(LegalStateRegistry, LegalStateRegistryArgs::__constructor(&admin));
        let client = LegalStateRegistryClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.record_state(
            &operator,
            &hash(&env, 1),
            &hash(&env, 2),
            &wallet,
            &hash(&env, 3),
            &asset,
            &20270630,
            &hash(&env, 4),
            &hash(&env, 5),
        );

        client.record_state(
            &operator,
            &hash(&env, 6),
            &hash(&env, 2),
            &wallet,
            &hash(&env, 3),
            &asset,
            &20270701,
            &hash(&env, 7),
            &hash(&env, 8),
        );

        let old_record = client.get_state(&hash(&env, 1));
        let new_record = client.get_state(&hash(&env, 6));

        assert_eq!(old_record.status, LegalStateStatus::Superseded);
        assert_eq!(new_record.status, LegalStateStatus::Active);
        assert_eq!(
            client.current_state_for_entitlement(&hash(&env, 3)),
            hash(&env, 6)
        );
    }

    #[test]
    fn rejects_duplicate_state_ids() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let wallet = Address::generate(&env);
        let asset = Address::generate(&env);

        let contract_id =
            env.register(LegalStateRegistry, LegalStateRegistryArgs::__constructor(&admin));
        let client = LegalStateRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.record_state(
            &operator,
            &hash(&env, 10),
            &hash(&env, 11),
            &wallet,
            &hash(&env, 12),
            &asset,
            &20270630,
            &hash(&env, 13),
            &hash(&env, 14),
        );

        let result = env.try_invoke_contract::<(), LegalStateRegistryError>(
            &contract_id,
            &Symbol::new(&env, "record_state"),
            vec![
                &env,
                operator.into_val(&env),
                hash(&env, 10).into_val(&env),
                hash(&env, 11).into_val(&env),
                wallet.into_val(&env),
                hash(&env, 12).into_val(&env),
                asset.into_val(&env),
                20270630u64.into_val(&env),
                hash(&env, 13).into_val(&env),
                hash(&env, 14).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(LegalStateRegistryError::StateExists))));
    }
}
