#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env,
};
use zkdtcc_types::{
    ComplianceActionType, ComplianceOperatorActionRecord, ParticipantFreezeState, PauseState,
};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const ACTION_DOMAIN: &[u8] = b"zkdtcc:compliance-action:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    GlobalPause,
    AssetPause(Address),
    ParticipantFreeze(BytesN<32>),
    OperatorAction(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ComplianceControlError {
    Unauthorized = 1,
    ActionNotFound = 2,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["global_pause_set"])]
pub struct GlobalPauseSetEvent {
    pub paused: bool,
    pub case_id: BytesN<32>,
}

#[contractevent(topics = ["asset_pause_set"])]
pub struct AssetPauseSetEvent {
    pub asset: Address,
    pub paused: bool,
    pub case_id: BytesN<32>,
}

#[contractevent(topics = ["participant_freeze_set"])]
pub struct ParticipantFreezeSetEvent {
    pub participant_id_hash: BytesN<32>,
    pub frozen: bool,
    pub case_id: BytesN<32>,
}

#[contractevent(topics = ["operator_action_recorded"])]
pub struct OperatorActionRecordedEvent {
    pub action_id: BytesN<32>,
    pub action_type: ComplianceActionType,
    pub case_id: BytesN<32>,
}

#[contract]
pub struct ComplianceControl;

#[contractimpl]
impl ComplianceControl {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), ComplianceControlError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_global_pause(
        env: Env,
        operator: Address,
        paused: bool,
        reason_code: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<PauseState, ComplianceControlError> {
        require_operator_auth(&env, &operator)?;
        let state = PauseState {
            paused,
            reason_code,
            case_id: case_id.clone(),
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::GlobalPause;
        env.storage().persistent().set(&key, &state);
        bump_persistent(&env, &key);
        bump_instance(&env);
        GlobalPauseSetEvent { paused, case_id }.publish(&env);
        Ok(state)
    }

    pub fn set_asset_pause(
        env: Env,
        operator: Address,
        asset: Address,
        paused: bool,
        reason_code: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<PauseState, ComplianceControlError> {
        require_operator_auth(&env, &operator)?;
        let state = PauseState {
            paused,
            reason_code,
            case_id: case_id.clone(),
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::AssetPause(asset.clone());
        env.storage().persistent().set(&key, &state);
        bump_persistent(&env, &key);
        bump_instance(&env);
        AssetPauseSetEvent {
            asset,
            paused,
            case_id,
        }
        .publish(&env);
        Ok(state)
    }

    pub fn set_participant_freeze(
        env: Env,
        operator: Address,
        participant_id_hash: BytesN<32>,
        frozen: bool,
        reason_code: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<ParticipantFreezeState, ComplianceControlError> {
        require_operator_auth(&env, &operator)?;
        let state = ParticipantFreezeState {
            frozen,
            reason_code,
            case_id: case_id.clone(),
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::ParticipantFreeze(participant_id_hash.clone());
        env.storage().persistent().set(&key, &state);
        bump_persistent(&env, &key);
        bump_instance(&env);
        ParticipantFreezeSetEvent {
            participant_id_hash,
            frozen,
            case_id,
        }
        .publish(&env);
        Ok(state)
    }

    pub fn record_operator_action(
        env: Env,
        operator: Address,
        action_type: ComplianceActionType,
        target_hash: BytesN<32>,
        reason_code: BytesN<32>,
        case_id: BytesN<32>,
        metadata_hash: BytesN<32>,
    ) -> Result<ComplianceOperatorActionRecord, ComplianceControlError> {
        require_operator_auth(&env, &operator)?;
        let action_id = derive_action_id(
            &env,
            &operator,
            &action_type,
            &target_hash,
            &reason_code,
            &case_id,
            &metadata_hash,
        );
        let action = ComplianceOperatorActionRecord {
            action_id: action_id.clone(),
            action_type: action_type.clone(),
            operator,
            target_hash,
            reason_code,
            case_id: case_id.clone(),
            metadata_hash,
            created_ledger: env.ledger().sequence(),
        };
        let key = DataKey::OperatorAction(action_id.clone());
        env.storage().persistent().set(&key, &action);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorActionRecordedEvent {
            action_id,
            action_type,
            case_id,
        }
        .publish(&env);
        Ok(action)
    }

    pub fn is_globally_paused(env: Env) -> bool {
        let key = DataKey::GlobalPause;
        let state: PauseState = env.storage().persistent().get(&key).unwrap_or(PauseState {
            paused: false,
            reason_code: zero_hash(&env),
            case_id: zero_hash(&env),
            updated_ledger: 0,
        });
        if state.updated_ledger != 0 {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        state.paused
    }

    pub fn is_asset_paused(env: Env, asset: Address) -> bool {
        let key = DataKey::AssetPause(asset);
        let state: PauseState = env.storage().persistent().get(&key).unwrap_or(PauseState {
            paused: false,
            reason_code: zero_hash(&env),
            case_id: zero_hash(&env),
            updated_ledger: 0,
        });
        if state.updated_ledger != 0 {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        state.paused
    }

    pub fn is_participant_frozen(env: Env, participant_id_hash: BytesN<32>) -> bool {
        let key = DataKey::ParticipantFreeze(participant_id_hash);
        let state: ParticipantFreezeState =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(ParticipantFreezeState {
                    frozen: false,
                    reason_code: zero_hash(&env),
                    case_id: zero_hash(&env),
                    updated_ledger: 0,
                });
        if state.updated_ledger != 0 {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        state.frozen
    }

    pub fn get_operator_action(
        env: Env,
        action_id: BytesN<32>,
    ) -> Result<ComplianceOperatorActionRecord, ComplianceControlError> {
        let key = DataKey::OperatorAction(action_id);
        let action = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(ComplianceControlError::ActionNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(action)
    }
}

fn derive_action_id(
    env: &Env,
    operator: &Address,
    action_type: &ComplianceActionType,
    target_hash: &BytesN<32>,
    reason_code: &BytesN<32>,
    case_id: &BytesN<32>,
    metadata_hash: &BytesN<32>,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(ACTION_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    append_address(&mut material, operator);
    material.extend_from_slice(&compliance_action_type_code(action_type).to_be_bytes());
    material.extend_from_slice(&target_hash.to_array());
    material.extend_from_slice(&reason_code.to_array());
    material.extend_from_slice(&case_id.to_array());
    material.extend_from_slice(&metadata_hash.to_array());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn compliance_action_type_code(action_type: &ComplianceActionType) -> u32 {
    match action_type {
        ComplianceActionType::Generic => 1,
        ComplianceActionType::ForcedTransferRequest => 2,
        ComplianceActionType::ForcedUnwindRequest => 3,
        ComplianceActionType::ClaimReversal => 4,
    }
}

fn zero_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0; 32])
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), ComplianceControlError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if stored_admin != *admin {
        return Err(ComplianceControlError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), ComplianceControlError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(ComplianceControlError::Unauthorized);
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

fn bump_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_TO);
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    #[test]
    fn records_pause_freeze_and_operator_action() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let asset = Address::generate(&env);
        let contract_id = env.register(ComplianceControl, ComplianceControlArgs::__constructor(&admin));
        let client = ComplianceControlClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.set_global_pause(&operator, &true, &hash(&env, 1), &hash(&env, 2));
        client.set_asset_pause(&operator, &asset, &true, &hash(&env, 3), &hash(&env, 4));
        client.set_participant_freeze(&operator, &hash(&env, 5), &true, &hash(&env, 6), &hash(&env, 7));
        let action = client.record_operator_action(
            &operator,
            &ComplianceActionType::Generic,
            &hash(&env, 8),
            &hash(&env, 9),
            &hash(&env, 10),
            &hash(&env, 11),
        );

        assert!(client.is_globally_paused());
        assert!(client.is_asset_paused(&asset));
        assert!(client.is_participant_frozen(&hash(&env, 5)));
        assert_eq!(client.get_operator_action(&action.action_id), action);
    }
}
