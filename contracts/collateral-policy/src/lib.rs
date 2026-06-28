#![no_std]

use soroban_sdk::{
    contract, contractclient, contracterror, contractevent, contractimpl, contracttype, Address,
    BytesN, Env,
};
use zkdtcc_types::{CollateralAssetPolicy, CollateralPolicySummary, ProofType};

#[contractclient(name = "AssetRegistryClient")]
pub trait AssetRegistryContract {
    fn is_supported_asset(env: Env, asset: Address) -> bool;
}

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    AssetRegistry,
    PolicyVersion,
    CurrentEpoch,
    RequiredMargin,
    AssetPolicy(Address),
    AcceptedVerifier(ProofType, BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum CollateralPolicyError {
    Unauthorized = 1,
    UnsupportedAsset = 2,
    InvalidHaircut = 3,
    InvalidPrice = 4,
    InvalidEpoch = 5,
    InvalidRequiredMargin = 6,
    AssetNotFound = 7,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["policy_updated"])]
pub struct PolicyUpdatedEvent {
    pub policy_version: u32,
    pub current_epoch: u64,
    pub required_margin: i128,
}

#[contractevent(topics = ["asset_policy_updated"])]
pub struct AssetPolicyUpdatedEvent {
    pub asset: Address,
    pub policy_version: u32,
}

#[contractevent(topics = ["accepted_verifier_updated"])]
pub struct AcceptedVerifierUpdatedEvent {
    pub proof_type: ProofType,
    pub verifier_id: BytesN<32>,
    pub enabled: bool,
    pub policy_version: u32,
}

#[contract]
pub struct CollateralPolicy;

#[contractimpl]
impl CollateralPolicy {
    pub fn __constructor(
        env: Env,
        admin: Address,
        asset_registry: Address,
        required_margin: i128,
        current_epoch: u64,
    ) -> Result<(), CollateralPolicyError> {
        if required_margin <= 0 {
            return Err(CollateralPolicyError::InvalidRequiredMargin);
        }
        if current_epoch == 0 {
            return Err(CollateralPolicyError::InvalidEpoch);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::AssetRegistry, &asset_registry);
        env.storage().instance().set(&DataKey::PolicyVersion, &1u32);
        env.storage().instance().set(&DataKey::CurrentEpoch, &current_epoch);
        env.storage()
            .instance()
            .set(&DataKey::RequiredMargin, &required_margin);
        bump_instance(&env);
        Ok(())
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), CollateralPolicyError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn set_global_policy(
        env: Env,
        operator: Address,
        required_margin: i128,
        current_epoch: u64,
    ) -> Result<u32, CollateralPolicyError> {
        require_operator_auth(&env, &operator)?;
        if required_margin <= 0 {
            return Err(CollateralPolicyError::InvalidRequiredMargin);
        }
        if current_epoch == 0 {
            return Err(CollateralPolicyError::InvalidEpoch);
        }

        env.storage()
            .instance()
            .set(&DataKey::RequiredMargin, &required_margin);
        env.storage().instance().set(&DataKey::CurrentEpoch, &current_epoch);
        let next_version = bump_policy_version(&env);

        PolicyUpdatedEvent {
            policy_version: next_version,
            current_epoch,
            required_margin,
        }
        .publish(&env);

        Ok(next_version)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_asset_policy(
        env: Env,
        operator: Address,
        asset: Address,
        decimals: u32,
        haircut_bps: u32,
        price: i128,
        price_epoch: u64,
        enabled: bool,
    ) -> Result<u32, CollateralPolicyError> {
        require_operator_auth(&env, &operator)?;
        validate_asset_input(&env, &asset, haircut_bps, price, price_epoch)?;

        let record = CollateralAssetPolicy {
            asset: asset.clone(),
            decimals,
            haircut_bps,
            price,
            price_epoch,
            enabled,
            updated_ledger: env.ledger().sequence(),
        };

        let key = DataKey::AssetPolicy(asset.clone());
        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        let next_version = bump_policy_version(&env);

        AssetPolicyUpdatedEvent {
            asset,
            policy_version: next_version,
        }
        .publish(&env);

        Ok(next_version)
    }

    pub fn set_accepted_verifier(
        env: Env,
        operator: Address,
        proof_type: ProofType,
        verifier_id: BytesN<32>,
        enabled: bool,
    ) -> Result<u32, CollateralPolicyError> {
        require_operator_auth(&env, &operator)?;
        let key = DataKey::AcceptedVerifier(proof_type.clone(), verifier_id.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        let next_version = bump_policy_version(&env);

        AcceptedVerifierUpdatedEvent {
            proof_type,
            verifier_id,
            enabled,
            policy_version: next_version,
        }
        .publish(&env);

        Ok(next_version)
    }

    pub fn get_policy_summary(env: Env) -> CollateralPolicySummary {
        bump_instance(&env);
        CollateralPolicySummary {
            policy_version: env.storage().instance().get(&DataKey::PolicyVersion).unwrap(),
            current_epoch: env.storage().instance().get(&DataKey::CurrentEpoch).unwrap(),
            required_margin: env.storage().instance().get(&DataKey::RequiredMargin).unwrap(),
        }
    }

    pub fn get_asset_policy(
        env: Env,
        asset: Address,
    ) -> Result<CollateralAssetPolicy, CollateralPolicyError> {
        let key = DataKey::AssetPolicy(asset);
        let record = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(CollateralPolicyError::AssetNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(record)
    }

    pub fn is_verifier_accepted(
        env: Env,
        proof_type: ProofType,
        verifier_id: BytesN<32>,
    ) -> bool {
        let key = DataKey::AcceptedVerifier(proof_type, verifier_id);
        let accepted = env.storage().persistent().get(&key).unwrap_or(false);
        if accepted {
            bump_persistent(&env, &key);
        }
        bump_instance(&env);
        accepted
    }
}

fn validate_asset_input(
    env: &Env,
    asset: &Address,
    haircut_bps: u32,
    price: i128,
    price_epoch: u64,
) -> Result<(), CollateralPolicyError> {
    if haircut_bps > 10_000 {
        return Err(CollateralPolicyError::InvalidHaircut);
    }
    if price <= 0 {
        return Err(CollateralPolicyError::InvalidPrice);
    }
    if price_epoch == 0 {
        return Err(CollateralPolicyError::InvalidEpoch);
    }

    let asset_registry: Address = env.storage().instance().get(&DataKey::AssetRegistry).unwrap();
    let client = AssetRegistryClient::new(env, &asset_registry);
    if !client.is_supported_asset(asset) {
        return Err(CollateralPolicyError::UnsupportedAsset);
    }
    Ok(())
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), CollateralPolicyError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(CollateralPolicyError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), CollateralPolicyError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(CollateralPolicyError::Unauthorized);
    }
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(())
}

fn bump_policy_version(env: &Env) -> u32 {
    let current: u32 = env.storage().instance().get(&DataKey::PolicyVersion).unwrap();
    let next = current.checked_add(1).unwrap();
    env.storage().instance().set(&DataKey::PolicyVersion, &next);
    bump_instance(env);
    next
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
    use soroban_sdk::{testutils::Address as _, vec, Address, BytesN, Env, IntoVal, Symbol};

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    fn setup_asset_registry(env: &Env) -> (Address, Address, Address) {
        env.mock_all_auths();
        let admin = Address::generate(env);
        let operator = Address::generate(env);
        let asset = Address::generate(env);
        let issuer = Address::generate(env);

        let registry_id = env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
        let client = asset_registry::AssetRegistryClient::new(env, &registry_id);
        client.set_operator(&admin, &operator, &true);
        client.register_asset(
            &operator,
            &asset,
            &hash(env, 1),
            &issuer,
            &zkdtcc_types::AssetClass::UsdcSac,
            &true,
            &true,
            &true,
            &true,
            &hash(env, 2),
            &hash(env, 3),
        );

        (admin, registry_id, asset)
    }

    #[test]
    fn updates_policy_and_asset_state() {
        let env = Env::default();
        let (admin, asset_registry_id, asset) = setup_asset_registry(&env);
        let operator = Address::generate(&env);

        let contract_id = env.register(
            CollateralPolicy,
            CollateralPolicyArgs::__constructor(&admin, &asset_registry_id, &1_000_000i128, &42u64),
        );
        let client = CollateralPolicyClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.upsert_asset_policy(&operator, &asset, &7u32, &8_500u32, &125_000i128, &42u64, &true);
        client.set_accepted_verifier(
            &operator,
            &ProofType::CollateralSufficiency,
            &hash(&env, 9),
            &true,
        );
        client.set_global_policy(&operator, &1_500_000i128, &43u64);

        let summary = client.get_policy_summary();
        assert_eq!(summary.required_margin, 1_500_000);
        assert_eq!(summary.current_epoch, 43);
        assert_eq!(summary.policy_version, 4);

        let asset_policy = client.get_asset_policy(&asset);
        assert_eq!(asset_policy.haircut_bps, 8_500);
        assert!(client.is_verifier_accepted(&ProofType::CollateralSufficiency, &hash(&env, 9)));
    }

    #[test]
    fn rejects_unsupported_asset() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let unknown_asset = Address::generate(&env);
        let registry_id = env.register(asset_registry::AssetRegistry, AssetRegistryArgs::__constructor(&admin));
        let contract_id = env.register(
            CollateralPolicy,
            CollateralPolicyArgs::__constructor(&admin, &registry_id, &1_000_000i128, &42u64),
        );
        let client = CollateralPolicyClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let result = env.try_invoke_contract::<u32, CollateralPolicyError>(
            &contract_id,
            &Symbol::new(&env, "upsert_asset_policy"),
            vec![
                &env,
                operator.into_val(&env),
                unknown_asset.into_val(&env),
                7u32.into_val(&env),
                9_000u32.into_val(&env),
                100i128.into_val(&env),
                42u64.into_val(&env),
                true.into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(CollateralPolicyError::UnsupportedAsset))));
    }

    #[test]
    fn rejects_invalid_parameters() {
        let env = Env::default();
        let (admin, asset_registry_id, asset) = setup_asset_registry(&env);
        let operator = Address::generate(&env);
        let contract_id = env.register(
            CollateralPolicy,
            CollateralPolicyArgs::__constructor(&admin, &asset_registry_id, &1_000_000i128, &42u64),
        );
        let client = CollateralPolicyClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);

        let haircut_result = env.try_invoke_contract::<u32, CollateralPolicyError>(
            &contract_id,
            &Symbol::new(&env, "upsert_asset_policy"),
            vec![
                &env,
                operator.into_val(&env),
                asset.into_val(&env),
                7u32.into_val(&env),
                10_001u32.into_val(&env),
                100i128.into_val(&env),
                42u64.into_val(&env),
                true.into_val(&env),
            ],
        );
        assert!(matches!(haircut_result, Err(Ok(CollateralPolicyError::InvalidHaircut))));

        let margin_result = env.try_invoke_contract::<u32, CollateralPolicyError>(
            &contract_id,
            &Symbol::new(&env, "set_global_policy"),
            vec![&env, operator.into_val(&env), 0i128.into_val(&env), 43u64.into_val(&env)],
        );
        assert!(matches!(margin_result, Err(Ok(CollateralPolicyError::InvalidRequiredMargin))));
    }
}
