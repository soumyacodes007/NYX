#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, BytesN, Env,
};
use zkdtcc_types::{AssetClass, AssetRecord, AssetStatus};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    Asset(Address),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AssetRegistryError {
    Unauthorized = 1,
    AssetExists = 2,
    AssetNotFound = 3,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["asset_registered"])]
pub struct AssetRegisteredEvent {
    pub asset: Address,
    pub status: AssetStatus,
}

#[contractevent(topics = ["asset_status"])]
pub struct AssetStatusEvent {
    pub asset: Address,
    pub status: AssetStatus,
}

#[contractevent(topics = ["asset_metadata"])]
pub struct AssetMetadataEvent {
    pub asset: Address,
    pub updated_ledger: u32,
}

#[contractevent(topics = ["asset_policy"])]
pub struct AssetPolicyEvent {
    pub asset: Address,
    pub updated_ledger: u32,
}

#[contract]
pub struct AssetRegistry;

#[contractimpl]
impl AssetRegistry {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), AssetRegistryError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn register_asset(
        env: Env,
        operator: Address,
        asset: Address,
        asset_id_hash: BytesN<32>,
        issuer: Address,
        asset_class: AssetClass,
        uses_sac: bool,
        requires_registered_wallets: bool,
        requires_issuer_auth: bool,
        clawback_enabled: bool,
        metadata_hash: BytesN<32>,
        issuer_policy_hash: BytesN<32>,
    ) -> Result<(), AssetRegistryError> {
        require_operator_auth(&env, &operator)?;
        let key = DataKey::Asset(asset.clone());
        if env.storage().persistent().has(&key) {
            return Err(AssetRegistryError::AssetExists);
        }

        let ledger = env.ledger().sequence();
        let record = AssetRecord {
            asset_id_hash,
            issuer,
            asset_class,
            status: AssetStatus::Active,
            uses_sac,
            requires_registered_wallets,
            requires_issuer_auth,
            clawback_enabled,
            metadata_hash,
            issuer_policy_hash,
            created_ledger: ledger,
            updated_ledger: ledger,
        };

        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);
        AssetRegisteredEvent {
            asset,
            status: record.status,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_status(
        env: Env,
        operator: Address,
        asset: Address,
        status: AssetStatus,
    ) -> Result<(), AssetRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_asset(&env, &asset)?;
        record.status = status;
        record.updated_ledger = env.ledger().sequence();
        save_asset(&env, &asset, &record);
        AssetStatusEvent {
            asset,
            status: record.status,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_metadata_hash(
        env: Env,
        operator: Address,
        asset: Address,
        metadata_hash: BytesN<32>,
    ) -> Result<(), AssetRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_asset(&env, &asset)?;
        record.metadata_hash = metadata_hash;
        record.updated_ledger = env.ledger().sequence();
        save_asset(&env, &asset, &record);
        AssetMetadataEvent {
            asset,
            updated_ledger: record.updated_ledger,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_policy_hash(
        env: Env,
        operator: Address,
        asset: Address,
        issuer_policy_hash: BytesN<32>,
    ) -> Result<(), AssetRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut record = load_asset(&env, &asset)?;
        record.issuer_policy_hash = issuer_policy_hash;
        record.updated_ledger = env.ledger().sequence();
        save_asset(&env, &asset, &record);
        AssetPolicyEvent {
            asset,
            updated_ledger: record.updated_ledger,
        }
        .publish(&env);
        Ok(())
    }

    pub fn get_asset(env: Env, asset: Address) -> Result<AssetRecord, AssetRegistryError> {
        load_asset(&env, &asset)
    }

    pub fn is_supported_asset(env: Env, asset: Address) -> bool {
        match load_asset(&env, &asset) {
            Ok(record) => record.status == AssetStatus::Active,
            Err(_) => false,
        }
    }
}

fn load_asset(env: &Env, asset: &Address) -> Result<AssetRecord, AssetRegistryError> {
    let key = DataKey::Asset(asset.clone());
    let record = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(AssetRegistryError::AssetNotFound)?;
    bump_persistent(env, &key);
    bump_instance(env);
    Ok(record)
}

fn save_asset(env: &Env, asset: &Address, record: &AssetRecord) {
    let key = DataKey::Asset(asset.clone());
    env.storage().persistent().set(&key, record);
    bump_persistent(env, &key);
    bump_instance(env);
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), AssetRegistryError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if &stored_admin != admin {
        return Err(AssetRegistryError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), AssetRegistryError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }

    let key = DataKey::Operator(operator.clone());
    let is_enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !is_enabled {
        return Err(AssetRegistryError::Unauthorized);
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
    fn registers_and_updates_assets() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let asset = Address::generate(&env);
        let issuer = Address::generate(&env);

        let contract_id = env.register(AssetRegistry, AssetRegistryArgs::__constructor(&admin));
        let client = AssetRegistryClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.register_asset(
            &operator,
            &asset,
            &hash(&env, 1),
            &issuer,
            &AssetClass::UsdcSac,
            &true,
            &true,
            &true,
            &true,
            &hash(&env, 2),
            &hash(&env, 3),
        );

        let record = client.get_asset(&asset);
        assert_eq!(record.asset_class, AssetClass::UsdcSac);
        assert_eq!(record.status, AssetStatus::Active);
        assert!(client.is_supported_asset(&asset));

        client.set_status(&operator, &asset, &AssetStatus::Suspended);
        assert!(!client.is_supported_asset(&asset));
    }

    #[test]
    fn rejects_duplicate_asset() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let asset = Address::generate(&env);
        let issuer = Address::generate(&env);

        let contract_id = env.register(AssetRegistry, AssetRegistryArgs::__constructor(&admin));
        let client = AssetRegistryClient::new(&env, &contract_id);
        client.set_operator(&admin, &operator, &true);
        client.register_asset(
            &operator,
            &asset,
            &hash(&env, 10),
            &issuer,
            &AssetClass::DtcEntitlement,
            &true,
            &true,
            &true,
            &false,
            &hash(&env, 11),
            &hash(&env, 12),
        );

        let result = env.try_invoke_contract::<(), AssetRegistryError>(
            &contract_id,
            &Symbol::new(&env, "register_asset"),
            vec![
                &env,
                operator.into_val(&env),
                asset.into_val(&env),
                hash(&env, 13).into_val(&env),
                issuer.into_val(&env),
                AssetClass::DtcEntitlement.into_val(&env),
                true.into_val(&env),
                true.into_val(&env),
                true.into_val(&env),
                false.into_val(&env),
                hash(&env, 14).into_val(&env),
                hash(&env, 15).into_val(&env),
            ],
        );

        assert!(matches!(result, Err(Ok(AssetRegistryError::AssetExists))));
    }
}
