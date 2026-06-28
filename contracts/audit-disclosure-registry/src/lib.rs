#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Bytes, BytesN,
    Env,
};
use zkdtcc_types::{
    DisclosureAccessReceipt, DisclosureBlob, DisclosureGrant, OperatorActionLinkRecord,
    ViewKeyCommitment,
};

const INSTANCE_BUMP_THRESHOLD: u32 = 17_280;
const INSTANCE_BUMP_TO: u32 = 518_400;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280;
const PERSISTENT_BUMP_TO: u32 = 518_400;
const GRANT_DOMAIN: &[u8] = b"zkdtcc:disclosure-grant:v1";
const RECEIPT_DOMAIN: &[u8] = b"zkdtcc:disclosure-access:v1";

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Operator(Address),
    Blob(BytesN<32>),
    Grant(BytesN<32>),
    ScopeGrant(BytesN<32>, Address),
    AccessReceipt(BytesN<32>),
    ViewKeyCommitment(BytesN<32>),
    OperatorActionLink(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum AuditDisclosureRegistryError {
    Unauthorized = 1,
    InvalidExpiry = 2,
    BlobNotFound = 3,
    GrantNotFound = 4,
    GrantInactive = 5,
    GrantExpired = 6,
    AccessReceiptNotFound = 7,
}

#[contractevent(topics = ["operator_set"])]
pub struct OperatorSetEvent {
    pub operator: Address,
    pub enabled: bool,
}

#[contractevent(topics = ["blob_registered"])]
pub struct BlobRegisteredEvent {
    pub blob_hash: BytesN<32>,
    pub owner_scope_hash: BytesN<32>,
}

#[contractevent(topics = ["grant_set"])]
pub struct GrantSetEvent {
    pub grant_id: BytesN<32>,
    pub grantee: Address,
    pub active: bool,
}

#[contractevent(topics = ["access_recorded"])]
pub struct AccessRecordedEvent {
    pub receipt_id: BytesN<32>,
    pub accessor: Address,
    pub scope_hash: BytesN<32>,
}

#[contractevent(topics = ["view_key_set"])]
pub struct ViewKeySetEvent {
    pub scope_hash: BytesN<32>,
}

#[contract]
pub struct AuditDisclosureRegistry;

#[contractimpl]
impl AuditDisclosureRegistry {
    pub fn __constructor(env: Env, admin: Address) {
        env.storage().instance().set(&DataKey::Admin, &admin);
        bump_instance(&env);
    }

    pub fn set_operator(
        env: Env,
        admin: Address,
        operator: Address,
        enabled: bool,
    ) -> Result<(), AuditDisclosureRegistryError> {
        require_admin_auth(&env, &admin)?;
        let key = DataKey::Operator(operator.clone());
        env.storage().persistent().set(&key, &enabled);
        bump_persistent(&env, &key);
        bump_instance(&env);
        OperatorSetEvent { operator, enabled }.publish(&env);
        Ok(())
    }

    pub fn register_blob(
        env: Env,
        operator: Address,
        blob_hash: BytesN<32>,
        blob_type: u32,
        owner_scope_hash: BytesN<32>,
        metadata_hash: BytesN<32>,
    ) -> Result<DisclosureBlob, AuditDisclosureRegistryError> {
        require_operator_auth(&env, &operator)?;
        let blob = DisclosureBlob {
            blob_hash: blob_hash.clone(),
            blob_type,
            owner_scope_hash: owner_scope_hash.clone(),
            metadata_hash,
            created_ledger: env.ledger().sequence(),
        };
        let key = DataKey::Blob(blob_hash.clone());
        env.storage().persistent().set(&key, &blob);
        bump_persistent(&env, &key);
        bump_instance(&env);
        BlobRegisteredEvent {
            blob_hash,
            owner_scope_hash,
        }
        .publish(&env);
        Ok(blob)
    }

    pub fn set_view_key_commitment(
        env: Env,
        operator: Address,
        scope_hash: BytesN<32>,
        commitment_hash: BytesN<32>,
    ) -> Result<ViewKeyCommitment, AuditDisclosureRegistryError> {
        require_operator_auth(&env, &operator)?;
        let record = ViewKeyCommitment {
            scope_hash: scope_hash.clone(),
            commitment_hash,
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::ViewKeyCommitment(scope_hash.clone());
        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);
        ViewKeySetEvent { scope_hash }.publish(&env);
        Ok(record)
    }

    pub fn grant(
        env: Env,
        operator: Address,
        scope_hash: BytesN<32>,
        grantee: Address,
        encrypted_key_hash: BytesN<32>,
        expiry_ledger: u32,
        purpose_code: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<DisclosureGrant, AuditDisclosureRegistryError> {
        require_operator_auth(&env, &operator)?;
        if env.ledger().sequence() >= expiry_ledger {
            return Err(AuditDisclosureRegistryError::InvalidExpiry);
        }
        let grant_id = derive_grant_id(&env, &scope_hash, &grantee);
        let grant = DisclosureGrant {
            grant_id: grant_id.clone(),
            scope_hash: scope_hash.clone(),
            grantee: grantee.clone(),
            encrypted_key_hash,
            purpose_code,
            case_id,
            expiry_ledger,
            active: true,
            created_ledger: env.ledger().sequence(),
            updated_ledger: env.ledger().sequence(),
        };
        let key = DataKey::Grant(grant_id.clone());
        let scope_key = DataKey::ScopeGrant(scope_hash, grantee.clone());
        env.storage().persistent().set(&key, &grant);
        env.storage().persistent().set(&scope_key, &grant_id);
        bump_persistent(&env, &key);
        bump_persistent(&env, &scope_key);
        bump_instance(&env);
        GrantSetEvent {
            grant_id,
            grantee,
            active: true,
        }
        .publish(&env);
        Ok(grant)
    }

    pub fn revoke_grant(
        env: Env,
        operator: Address,
        grant_id: BytesN<32>,
        case_id: BytesN<32>,
    ) -> Result<DisclosureGrant, AuditDisclosureRegistryError> {
        require_operator_auth(&env, &operator)?;
        let mut grant = load_grant(&env, &grant_id)?;
        grant.active = false;
        grant.case_id = case_id;
        grant.updated_ledger = env.ledger().sequence();
        let key = DataKey::Grant(grant_id.clone());
        env.storage().persistent().set(&key, &grant);
        bump_persistent(&env, &key);
        bump_instance(&env);
        GrantSetEvent {
            grant_id,
            grantee: grant.grantee.clone(),
            active: false,
        }
        .publish(&env);
        Ok(grant)
    }

    pub fn record_access(
        env: Env,
        accessor: Address,
        scope_hash: BytesN<32>,
        purpose_code: BytesN<32>,
        case_id: BytesN<32>,
        blob_hash: BytesN<32>,
    ) -> Result<DisclosureAccessReceipt, AuditDisclosureRegistryError> {
        accessor.require_auth();
        ensure_blob_exists(&env, &blob_hash)?;
        let grant_id = load_scope_grant_id(&env, &scope_hash, &accessor)?;
        let grant = load_grant(&env, &grant_id)?;
        if !grant.active {
            return Err(AuditDisclosureRegistryError::GrantInactive);
        }
        if env.ledger().sequence() > grant.expiry_ledger {
            return Err(AuditDisclosureRegistryError::GrantExpired);
        }
        let receipt_id = derive_access_receipt_id(
            &env,
            &scope_hash,
            &accessor,
            &purpose_code,
            &case_id,
            &blob_hash,
        );
        let receipt = DisclosureAccessReceipt {
            receipt_id: receipt_id.clone(),
            scope_hash: scope_hash.clone(),
            accessor: accessor.clone(),
            purpose_code,
            case_id,
            blob_hash,
            access_ledger: env.ledger().sequence(),
        };
        let key = DataKey::AccessReceipt(receipt_id.clone());
        env.storage().persistent().set(&key, &receipt);
        bump_persistent(&env, &key);
        bump_instance(&env);
        AccessRecordedEvent {
            receipt_id,
            accessor,
            scope_hash,
        }
        .publish(&env);
        Ok(receipt)
    }

    pub fn link_operator_action(
        env: Env,
        operator: Address,
        action_id: BytesN<32>,
        scope_hash: BytesN<32>,
        blob_hash: BytesN<32>,
    ) -> Result<OperatorActionLinkRecord, AuditDisclosureRegistryError> {
        require_operator_auth(&env, &operator)?;
        ensure_blob_exists(&env, &blob_hash)?;
        let record = OperatorActionLinkRecord {
            action_id: action_id.clone(),
            scope_hash,
            blob_hash,
            linked_ledger: env.ledger().sequence(),
        };
        let key = DataKey::OperatorActionLink(action_id);
        env.storage().persistent().set(&key, &record);
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(record)
    }

    pub fn has_active_grant(env: Env, scope_hash: BytesN<32>, grantee: Address) -> bool {
        let grant_id = load_scope_grant_id(&env, &scope_hash, &grantee);
        if let Ok(id) = grant_id {
            if let Ok(grant) = load_grant(&env, &id) {
                bump_instance(&env);
                return grant.active && env.ledger().sequence() <= grant.expiry_ledger;
            }
        }
        bump_instance(&env);
        false
    }

    pub fn get_blob(
        env: Env,
        blob_hash: BytesN<32>,
    ) -> Result<DisclosureBlob, AuditDisclosureRegistryError> {
        let key = DataKey::Blob(blob_hash);
        let blob = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(AuditDisclosureRegistryError::BlobNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(blob)
    }

    pub fn get_grant(
        env: Env,
        grant_id: BytesN<32>,
    ) -> Result<DisclosureGrant, AuditDisclosureRegistryError> {
        let grant = load_grant(&env, &grant_id)?;
        bump_instance(&env);
        Ok(grant)
    }

    pub fn get_access_receipt(
        env: Env,
        receipt_id: BytesN<32>,
    ) -> Result<DisclosureAccessReceipt, AuditDisclosureRegistryError> {
        let key = DataKey::AccessReceipt(receipt_id);
        let receipt = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(AuditDisclosureRegistryError::AccessReceiptNotFound)?;
        bump_persistent(&env, &key);
        bump_instance(&env);
        Ok(receipt)
    }
}

fn ensure_blob_exists(env: &Env, blob_hash: &BytesN<32>) -> Result<(), AuditDisclosureRegistryError> {
    let key = DataKey::Blob(blob_hash.clone());
    if !env.storage().persistent().has(&key) {
        return Err(AuditDisclosureRegistryError::BlobNotFound);
    }
    bump_persistent(env, &key);
    Ok(())
}

fn load_scope_grant_id(
    env: &Env,
    scope_hash: &BytesN<32>,
    grantee: &Address,
) -> Result<BytesN<32>, AuditDisclosureRegistryError> {
    let key = DataKey::ScopeGrant(scope_hash.clone(), grantee.clone());
    let grant_id = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(AuditDisclosureRegistryError::GrantNotFound)?;
    bump_persistent(env, &key);
    Ok(grant_id)
}

fn load_grant(
    env: &Env,
    grant_id: &BytesN<32>,
) -> Result<DisclosureGrant, AuditDisclosureRegistryError> {
    let key = DataKey::Grant(grant_id.clone());
    let grant = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(AuditDisclosureRegistryError::GrantNotFound)?;
    bump_persistent(env, &key);
    Ok(grant)
}

fn derive_grant_id(env: &Env, scope_hash: &BytesN<32>, grantee: &Address) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(GRANT_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&scope_hash.to_array());
    append_address(&mut material, grantee);
    env.crypto().sha256(&material).into()
}

fn derive_access_receipt_id(
    env: &Env,
    scope_hash: &BytesN<32>,
    accessor: &Address,
    purpose_code: &BytesN<32>,
    case_id: &BytesN<32>,
    blob_hash: &BytesN<32>,
) -> BytesN<32> {
    let mut material = Bytes::new(env);
    material.extend_from_slice(RECEIPT_DOMAIN);
    append_address(&mut material, &env.current_contract_address());
    material.extend_from_slice(&scope_hash.to_array());
    append_address(&mut material, accessor);
    material.extend_from_slice(&purpose_code.to_array());
    material.extend_from_slice(&case_id.to_array());
    material.extend_from_slice(&blob_hash.to_array());
    env.crypto().sha256(&material).into()
}

fn append_address(material: &mut Bytes, address: &Address) {
    let address_bytes = address.to_string().to_bytes();
    material.extend_from_slice(&address_bytes.len().to_be_bytes());
    material.append(&address_bytes);
}

fn require_admin_auth(env: &Env, admin: &Address) -> Result<(), AuditDisclosureRegistryError> {
    admin.require_auth();
    let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if stored_admin != *admin {
        return Err(AuditDisclosureRegistryError::Unauthorized);
    }
    bump_instance(env);
    Ok(())
}

fn require_operator_auth(env: &Env, operator: &Address) -> Result<(), AuditDisclosureRegistryError> {
    operator.require_auth();
    let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
    if admin == *operator {
        bump_instance(env);
        return Ok(());
    }
    let key = DataKey::Operator(operator.clone());
    let enabled = env.storage().persistent().get(&key).unwrap_or(false);
    if !enabled {
        return Err(AuditDisclosureRegistryError::Unauthorized);
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
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, BytesN, Env};

    fn hash(env: &Env, value: u8) -> BytesN<32> {
        BytesN::from_array(env, &[value; 32])
    }

    #[test]
    fn grants_and_records_access() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|ledger| ledger.sequence_number = 50);

        let admin = Address::generate(&env);
        let operator = Address::generate(&env);
        let auditor = Address::generate(&env);
        let contract_id =
            env.register(AuditDisclosureRegistry, AuditDisclosureRegistryArgs::__constructor(&admin));
        let client = AuditDisclosureRegistryClient::new(&env, &contract_id);

        client.set_operator(&admin, &operator, &true);
        client.register_blob(&operator, &hash(&env, 1), &7u32, &hash(&env, 2), &hash(&env, 3));
        let grant = client.grant(
            &operator,
            &hash(&env, 4),
            &auditor,
            &hash(&env, 5),
            &100u32,
            &hash(&env, 6),
            &hash(&env, 7),
        );
        let receipt = client.record_access(
            &auditor,
            &hash(&env, 4),
            &hash(&env, 8),
            &hash(&env, 9),
            &hash(&env, 1),
        );

        assert!(client.has_active_grant(&hash(&env, 4), &auditor));
        assert_eq!(client.get_grant(&grant.grant_id), grant);
        assert_eq!(client.get_access_receipt(&receipt.receipt_id), receipt);

        client.revoke_grant(&operator, &grant.grant_id, &hash(&env, 10));
        assert!(!client.has_active_grant(&hash(&env, 4), &auditor));
    }
}
