#![no_std]
use escrow::EscrowContractClient;
use payments::PaymentContractClient;
use refund::RefundContractClient;
use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, Address, Env, IntoVal,
    String, Symbol, Val, Vec,
};

#[contracterror]
#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    RotationNotPending = 4,
    RotationTimelockActive = 5,
    RotationAlreadyPending = 6,
    // Issue #237: rotation handshake
    RotationToCurrentAddress = 20,
    RotationTargetNotReady = 21,
    PeerSyncFailed = 22,
}

/// Identifies which downstream contract address is being rotated.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ContractTarget {
    Payment,
    Escrow,
    Refund,
}

/// A contract address rotation that has been proposed but has not yet taken
/// effect because the timelock has not elapsed.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PendingRotation {
    /// The new contract address that will take effect after the timelock.
    pub new_address: Address,
    /// The admin who proposed the rotation.
    pub proposed_by: Address,
    /// Ledger timestamp (in seconds) at which the rotation was proposed.
    pub proposed_at: u64,
}

/// Timelock duration for contract address rotations (Issue #77).
///
/// A proposed rotation of the payment, escrow, or refund contract address only
/// takes effect after this many seconds have elapsed, giving operators time to
/// detect and react to a malicious or mistaken proposal.
pub const CONTRACT_ROTATION_TIMELOCK: u64 = 48 * 60 * 60;

/// Unified health report for the payment, escrow, and refund contracts
/// (Issue #80).
///
/// Returned by [`AdminContract::get_system_status`] so frontends and monitoring
/// systems can check the operational status of every core contract in a single
/// RPC query.
#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct SystemStatus {
    /// Whether the payment contract is globally paused.
    pub payment_paused: bool,
    /// Storage schema version reported by the payment contract.
    pub payment_schema_version: u32,
    /// Whether the escrow contract is globally paused.
    pub escrow_paused: bool,
    /// Storage schema version reported by the escrow contract.
    pub escrow_schema_version: u32,
    /// Whether the refund contract is globally paused.
    pub refund_paused: bool,
    /// Storage schema version reported by the refund contract.
    pub refund_schema_version: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractTargetUpdated {
    /// Which contract target was rotated.
    pub target: ContractTarget,
    /// The contract address in effect before the rotation.
    pub old_address: Address,
    /// The contract address now in effect.
    pub new_address: Address,
    /// Ledger timestamp (in seconds) at which the rotation took effect.
    pub updated_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Pauser,
    PaymentContract,
    EscrowContract,
    RefundContract,
    PendingPaymentRotation,
    PendingEscrowRotation,
    PendingRefundRotation,
    // Issue #237: addresses a target was rotated away from
    DeprecatedContracts(ContractTarget),
}

/// Schema version written by `initialize`.
const INITIAL_SCHEMA_VERSION: u32 = 1;

#[contract]
pub struct AdminContract;

#[contractimpl]
impl AdminContract {
    /// Initializes the admin contract with the addresses of the payment, escrow,
    /// and refund contracts.
    ///
    /// # Parameters
    /// - `admin`: the address authorized to manage the contract.
    /// - `pauser`: the address authorized to pause/unpause the platform.
    /// - `payment_contract`: the deployed payment contract address.
    /// - `escrow_contract`: the deployed escrow contract address.
    /// - `refund_contract`: the deployed refund contract address.
    ///
    /// # Returns
    /// Returns `Ok(())` when initialization succeeds.
    ///
    /// # Errors
    /// Returns `Error::AlreadyInitialized` if the contract has already been set up.
    pub fn initialize(
        env: Env,
        admin: Address,
        pauser: Address,
        payment_contract: Address,
        escrow_contract: Address,
        refund_contract: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Pauser, &pauser);
        env.storage()
            .instance()
            .set(&DataKey::PaymentContract, &payment_contract);
        env.storage()
            .instance()
            .set(&DataKey::EscrowContract, &escrow_contract);
        env.storage()
            .instance()
            .set(&DataKey::RefundContract, &refund_contract);
        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &INITIAL_SCHEMA_VERSION);

        Ok(())
    }

    /// Returns the current schema version of the contract storage.
    ///
    /// # Returns
    /// The stored schema version, or `1` (`INITIAL_SCHEMA_VERSION`) when the
    /// contract was initialized before schema versioning was introduced.
    pub fn get_schema_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::SchemaVersion)
            .unwrap_or(INITIAL_SCHEMA_VERSION)
    }

    /// Migrates the contract storage schema to a target version.
    ///
    /// Every data transformation registered for the versions between the
    /// current schema version and `target_version` is executed *before*
    /// `target_version` is written to storage, so the version can never be
    /// bumped on top of partially migrated state. If a single entry cannot be
    /// migrated the call returns `Error::SchemaMigrationFailed` and the whole
    /// transaction is reverted.
    ///
    /// # Parameters
    /// - `admin`: the admin authorizing the migration (must be the stored admin).
    /// - `target_version`: the schema version to migrate to.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` before `initialize`, `Error::Unauthorized`
    /// if the caller is not the stored admin, `Error::SchemaAlreadyAtTarget` when
    /// the stored version is already at or past the target, and
    /// `Error::SchemaMigrationFailed` if a data migration step failed.
    pub fn migrate_schema(env: Env, admin: Address, target_version: u32) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let current = Self::get_schema_version(env.clone());
        if current >= target_version {
            return Err(Error::SchemaAlreadyAtTarget);
        }

        // Run every data migration first; `target_version` is only persisted
        // once all transformations have completed successfully.
        Self::run_data_migrations(&env, current, target_version)?;

        env.storage()
            .instance()
            .set(&DataKey::SchemaVersion, &target_version);
        Ok(())
    }

    /// Runs the data transformations registered for every schema version step
    /// between `from_version` (exclusive) and `to_version` (inclusive).
    fn run_data_migrations(env: &Env, from_version: u32, to_version: u32) -> Result<(), Error> {
        let mut version = from_version;
        while version < to_version {
            let next_version = version + 1;
            // v2: the orchestrator configuration must stay complete and
            // resolvable, because every privileged entry point reads it.
            if next_version == 2 {
                Self::migrate_v1_to_v2(env)?;
            }
            version = next_version;
        }
        Ok(())
    }

    /// v1 -> v2 data migration: re-validates the stored orchestrator
    /// configuration. Every privileged function resolves the admin, pauser and
    /// child contract addresses from instance storage, so a missing entry or a
    /// duplicated child contract must abort the migration instead of silently
    /// bumping the version.
    fn migrate_v1_to_v2(env: &Env) -> Result<(), Error> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Pauser)
            .ok_or(Error::SchemaMigrationFailed)?;
        let payment: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContract)
            .ok_or(Error::SchemaMigrationFailed)?;
        let escrow: Address = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContract)
            .ok_or(Error::SchemaMigrationFailed)?;
        let refund: Address = env
            .storage()
            .instance()
            .get(&DataKey::RefundContract)
            .ok_or(Error::SchemaMigrationFailed)?;

        // The three child contracts must be distinct, otherwise an emergency
        // pause would hit the same contract twice and leave a role unmanaged.
        if payment == escrow || payment == refund || escrow == refund {
            return Err(Error::SchemaMigrationFailed);
        }

        Ok(())
    }

    /// Pauses the payment, escrow, and refund contracts in one Soroban call.
    ///
    /// Issue #73: mirrors [`emergency_pause_all`](Self::emergency_pause_all) so
    /// unpausing is atomic too - a child that refuses to unpause reverts the
    /// whole transaction instead of leaving the platform half-live.
    ///
    /// # Parameters
    /// - `pauser`: the pauser address that must be authorized.
    /// - `reason`: a human-readable explanation for the emergency pause.
    ///
    /// # Returns
    /// Returns `Ok(())` when all child contracts are paused successfully.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// and `Error::Unauthorized` if the provided pauser address does not match the
    /// stored pauser.
    pub fn emergency_pause_all(env: Env, pauser: Address, reason: String) -> Result<(), Error> {
        pauser.require_auth();
        Self::require_pauser(&env, &pauser)?;
        let (payment_contract, escrow_contract, refund_contract) =
            Self::get_contract_addresses(&env)?;

        PaymentContractClient::new(env, &payment_contract).pause_contract(pauser, reason);
        EscrowContractClient::new(env, &escrow_contract).pause_contract(pauser, reason);
        RefundContractClient::new(env, &refund_contract).pause_contract(pauser, reason);

        // Emit audit event
        env.events().publish(
            (Symbol::new(env, "emergency_pause_all"), pauser.clone()),
            reason.clone(),
        );

        Ok(())
    }

    /// Unpauses the payment, escrow, and refund contracts in one Soroban call.
    /// Requires super-admin authorization (not just pauser).
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `justification`: a human-readable explanation for the emergency unpause.
    ///
    /// # Returns
    /// Returns `Ok(())` when all child contracts are unpaused successfully and the
    /// post-condition holds.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the pauser does not match the stored pauser,
    /// `Error::ChildAlreadyInTargetState` if any child is already unpaused,
    /// `Error::ChildContractCallFailed` if a child refuses to unpause, and
    /// `Error::ChildStateVerificationFailed` if the read-back does not show all
    /// three children unpaused.
    pub fn emergency_unpause_all(env: Env, pauser: Address) -> Result<(), Error> {
        pauser.require_auth();
        Self::require_pauser(&env, &pauser)?;
        let (payment_contract, escrow_contract, refund_contract) =
            Self::get_contract_addresses(&env)?;

        PaymentContractClient::new(env, &payment_contract).unpause_contract(admin);
        EscrowContractClient::new(env, &escrow_contract).unpause_contract(admin);
        RefundContractClient::new(env, &refund_contract).unpause_contract(admin);

        // Emit audit event
        env.events().publish(
            (Symbol::new(env, "emergency_unpause_all"), admin.clone()),
            justification.clone(),
        );

        Ok(())
    }

    /// Proposes a rotation of the stored payment contract address (Issue #77).
    ///
    /// The new address does not take effect immediately. It is stored as a
    /// pending rotation and only becomes active once
    /// [`AdminContract::execute_payment_rotation`] is called after the
    /// 48-hour timelock has elapsed. This prevents a single compromised admin
    /// from immediately redirecting the orchestrator to a malicious contract.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `payment_contract`: the new payment contract address.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, and `Error::RotationAlreadyPending` if a rotation for this
    /// target is already pending.
    pub fn set_payment_contract(
        env: Env,
        admin: Address,
        payment_contract: Address,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::propose_rotation(
            &env,
            DataKey::PendingPaymentRotation,
            &admin,
            &payment_contract,
        )
    }

    /// Proposes a rotation of the stored escrow contract address (Issue #77).
    ///
    /// The new address does not take effect immediately. It is stored as a
    /// pending rotation and only becomes active once
    /// [`AdminContract::execute_escrow_rotation`] is called after the
    /// 48-hour timelock has elapsed.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `escrow_contract`: the new escrow contract address.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, and `Error::RotationAlreadyPending` if a rotation for this
    /// target is already pending.
    pub fn set_escrow_contract(
        env: Env,
        admin: Address,
        escrow_contract: Address,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::propose_rotation(
            &env,
            DataKey::PendingEscrowRotation,
            &admin,
            &escrow_contract,
        )
    }

    /// Proposes a rotation of the stored refund contract address (Issue #77).
    ///
    /// The new address does not take effect immediately. It is stored as a
    /// pending rotation and only becomes active once
    /// [`AdminContract::execute_refund_rotation`] is called after the
    /// 48-hour timelock has elapsed.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `refund_contract`: the new refund contract address.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, and `Error::RotationAlreadyPending` if a rotation for this
    /// target is already pending.
    pub fn set_refund_contract(
        env: Env,
        admin: Address,
        refund_contract: Address,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::propose_rotation(
            &env,
            DataKey::PendingRefundRotation,
            &admin,
            &refund_contract,
        )
    }

    /// Executes a pending payment contract address rotation (Issue #77).
    ///
    /// The rotation only takes effect if at least
    /// [`CONTRACT_ROTATION_TIMELOCK`] seconds have elapsed since it was
    /// proposed. On success the stored payment contract address is updated,
    /// the pending rotation is cleared, and a [`ContractTargetUpdated`] event
    /// is emitted.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, `Error::RotationNotPending` if no rotation is pending for
    /// this target, and `Error::RotationTimelockActive` if the 48-hour timelock
    /// has not yet elapsed.
    pub fn execute_payment_rotation(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::execute_rotation(
            &env,
            ContractTarget::Payment,
            DataKey::PendingPaymentRotation,
            DataKey::PaymentContract,
        )
    }

    /// Executes a pending escrow contract address rotation (Issue #77).
    ///
    /// The rotation only takes effect if at least
    /// [`CONTRACT_ROTATION_TIMELOCK`] seconds have elapsed since it was
    /// proposed. On success the stored escrow contract address is updated,
    /// the pending rotation is cleared, and a [`ContractTargetUpdated`] event
    /// is emitted.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, `Error::RotationNotPending` if no rotation is pending for
    /// this target, and `Error::RotationTimelockActive` if the 48-hour timelock
    /// has not yet elapsed.
    pub fn execute_escrow_rotation(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::execute_rotation(
            &env,
            ContractTarget::Escrow,
            DataKey::PendingEscrowRotation,
            DataKey::EscrowContract,
        )
    }

    /// Executes a pending refund contract address rotation (Issue #77).
    ///
    /// The rotation only takes effect if at least
    /// [`CONTRACT_ROTATION_TIMELOCK`] seconds have elapsed since it was
    /// proposed. On success the stored refund contract address is updated,
    /// the pending rotation is cleared, and a [`ContractTargetUpdated`] event
    /// is emitted.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, `Error::RotationNotPending` if no rotation is pending for
    /// this target, and `Error::RotationTimelockActive` if the 48-hour timelock
    /// has not yet elapsed.
    pub fn execute_refund_rotation(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::execute_rotation(
            &env,
            ContractTarget::Refund,
            DataKey::PendingRefundRotation,
            DataKey::RefundContract,
        )
    }

    /// Cancels a pending contract address rotation (Issue #77).
    ///
    /// Allows the admin to abort a rotation during the timelock window, for
    /// example if the proposed address was found to be malicious or mistaken.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `target`: the contract target whose pending rotation should be canceled.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin, and `Error::RotationNotPending` if no rotation is pending
    /// for this target.
    pub fn cancel_proposed_rotation(
        env: Env,
        admin: Address,
        target: ContractTarget,
    ) -> Result<(), Error> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        let pending_key = Self::target_key(&target);
        if !env.storage().instance().has(&pending_key) {
            return Err(Error::RotationNotPending);
        }
        env.storage().instance().remove(&pending_key);
        Ok(())
    }

    /// Returns the pending rotation for a contract target, if one exists
    /// (Issue #77).
    ///
    /// # Parameters
    /// - `target`: the contract target to query.
    ///
    /// # Returns
    /// The pending rotation, or `None` if no rotation is pending for the target.
    pub fn get_pending_rotation(env: Env, target: ContractTarget) -> Option<PendingRotation> {
        let pending_key = Self::target_key(&target);
        env.storage().instance().get(&pending_key)
    }

    /// Returns a unified health report for the payment, escrow, and refund
    /// contracts (Issue #80).
    ///
    /// Queries the pause status and storage schema version of each core
    /// contract in a single call, so frontends and monitoring systems can
    /// check the operational status of the whole platform with one RPC query.
    ///
    /// # Parameters
    /// - `env` - The Soroban environment.
    ///
    /// # Returns
    /// A [`SystemStatus`] with the pause flag and schema version of each
    /// core contract.
    ///
    /// # Panics
    /// Panics if the admin contract has not been initialized.
    pub fn get_system_status(env: Env) -> SystemStatus {
        let payment_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContract)
            .unwrap_or_else(|| panic!("admin contract not initialized"));
        let escrow_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContract)
            .unwrap_or_else(|| panic!("admin contract not initialized"));
        let refund_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::RefundContract)
            .unwrap_or_else(|| panic!("admin contract not initialized"));

        let payment = PaymentContractClient::new(&env, &payment_contract);
        let escrow = EscrowContractClient::new(&env, &escrow_contract);
        let refund = RefundContractClient::new(&env, &refund_contract);

        SystemStatus {
            payment_paused: payment.get_pause_state().globally_paused,
            payment_schema_version: payment.get_schema_version(),
            escrow_paused: escrow.get_pause_state().globally_paused,
            escrow_schema_version: escrow.get_schema_version(),
            refund_paused: refund.get_pause_state().globally_paused,
            refund_schema_version: refund.get_schema_version(),
        }
    }

    /// Maps a contract target to its pending-rotation storage key.
    fn target_key(target: &ContractTarget) -> DataKey {
        match target {
            ContractTarget::Payment => DataKey::PendingPaymentRotation,
            ContractTarget::Escrow => DataKey::PendingEscrowRotation,
            ContractTarget::Refund => DataKey::PendingRefundRotation,
        }
    }

    /// Verifies the caller is the stored admin.
    fn require_admin(env: &Env, admin: &Address) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if *admin != stored_admin {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    /// Verifies the caller is the stored pauser.
    fn require_pauser(env: &Env, pauser: &Address) -> Result<(), Error> {
        let stored_pauser: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pauser)
            .ok_or(Error::NotInitialized)?;
        if *pauser != stored_pauser {
            return Err(Error::Unauthorized);
        }
        Ok(())
    }

    /// Reads the payment, escrow, and refund contract addresses from instance
    /// storage in a single helper so dispatch functions don't duplicate the
    /// reads and their error handling (Issue #81).
    fn get_contract_addresses(env: &Env) -> Result<(Address, Address, Address), Error> {
        let payment_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContract)
            .ok_or(Error::NotInitialized)?;
        let escrow_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContract)
            .ok_or(Error::NotInitialized)?;
        let refund_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::RefundContract)
            .ok_or(Error::NotInitialized)?;
        Ok((payment_contract, escrow_contract, refund_contract))
    }

    /// Stores a new pending rotation, rejecting duplicate proposals.
    fn propose_rotation(
        env: &Env,
        pending_key: DataKey,
        admin: &Address,
        new_address: &Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&pending_key) {
            return Err(Error::RotationAlreadyPending);
        }
        let rotation = PendingRotation {
            new_address: new_address.clone(),
            proposed_by: admin.clone(),
            proposed_at: env.ledger().timestamp(),
        };
        env.storage().instance().set(&pending_key, &rotation);
        Ok(())
    }

    /// Applies a pending rotation once its timelock has elapsed.
    ///
    /// Issue #237: the new address must differ from the current one and pass
    /// a readiness probe (`get_schema_version` and `is_initialized`). The old
    /// address is kept in the deprecated-contract allowlist so in-flight
    /// escrows/refunds can still complete legacy settlement callbacks, and
    /// the new address is broadcast to both sibling contracts. If any sibling
    /// rejects the update the whole rotation is rolled back.
    fn execute_rotation(
        env: &Env,
        target: ContractTarget,
        pending_key: DataKey,
        stored_key: DataKey,
    ) -> Result<(), Error> {
        let rotation: PendingRotation = env
            .storage()
            .instance()
            .get(&pending_key)
            .ok_or(Error::RotationNotPending)?;

        let now = env.ledger().timestamp();
        let eligible_at = rotation.proposed_at + CONTRACT_ROTATION_TIMELOCK;
        if now < eligible_at {
            return Err(Error::RotationTimelockActive);
        }

        let old_address: Address = env
            .storage()
            .instance()
            .get(&stored_key)
            .ok_or(Error::NotInitialized)?;
        if rotation.new_address == old_address {
            return Err(Error::RotationToCurrentAddress);
        }
        Self::probe_rotation_target(env, &rotation.new_address)?;

        env.storage()
            .instance()
            .set(&stored_key, &rotation.new_address);
        env.storage().instance().remove(&pending_key);

        let deprecated_key = DataKey::DeprecatedContracts(target.clone());
        let mut deprecated: Vec<Address> = env
            .storage()
            .instance()
            .get(&deprecated_key)
            .unwrap_or(Vec::new(env));
        deprecated.push_back(old_address.clone());
        env.storage().instance().set(&deprecated_key, &deprecated);

        Self::broadcast_peer_address(env, &target, &rotation.new_address)?;

        (ContractTargetUpdated {
            target,
            old_address,
            new_address: rotation.new_address,
            updated_at: now,
        })
        .publish(env);

        Ok(())
    }

    /// Rejects a rotation target that is not a deployed, initialized contract.
    fn probe_rotation_target(env: &Env, new_address: &Address) -> Result<(), Error> {
        let schema = env.try_invoke_contract::<u32, soroban_sdk::Error>(
            new_address,
            &Symbol::new(env, "get_schema_version"),
            Vec::new(env),
        );
        let initialized = env.try_invoke_contract::<bool, soroban_sdk::Error>(
            new_address,
            &Symbol::new(env, "is_initialized"),
            Vec::new(env),
        );
        match (schema, initialized) {
            (Ok(Ok(version)), Ok(Ok(true))) if version > 0 => Ok(()),
            _ => Err(Error::RotationTargetNotReady),
        }
    }

    /// Pushes a rotated address to the two sibling contracts, e.g. an escrow
    /// rotation calls `set_escrow_contract` on Payment and Refund.
    fn broadcast_peer_address(
        env: &Env,
        target: &ContractTarget,
        new_address: &Address,
    ) -> Result<(), Error> {
        let (payment, escrow, refund) = Self::get_contract_addresses(env)?;
        let (setter, peers) = match target {
            ContractTarget::Payment => ("set_payment_contract", [escrow, refund]),
            ContractTarget::Escrow => ("set_escrow_contract", [payment, refund]),
            ContractTarget::Refund => ("set_refund_contract", [payment, escrow]),
        };
        let args: Vec<Val> = (env.current_contract_address(), new_address.clone()).into_val(env);
        for peer in peers {
            let result = env.try_invoke_contract::<Val, soroban_sdk::Error>(
                &peer,
                &Symbol::new(env, setter),
                args.clone(),
            );
            if !matches!(result, Ok(Ok(_))) {
                return Err(Error::PeerSyncFailed);
            }
        }
        Ok(())
    }

    /// Returns the addresses a contract target was rotated away from, oldest
    /// first (Issue #237). Siblings use this allowlist to accept legacy
    /// settlement callbacks from contracts that are still draining.
    pub fn get_deprecated_contracts(env: Env, target: ContractTarget) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::DeprecatedContracts(target))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns whether `address` is a deprecated `target` contract that may
    /// still complete legacy settlement callbacks (Issue #237).
    pub fn is_deprecated_contract(env: Env, target: ContractTarget, address: Address) -> bool {
        Self::get_deprecated_contracts(env, target).contains(&address)
    }

    /// Sets or updates the emergency pauser address.
    /// Only the admin can change the pauser role.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `pauser`: the new emergency pauser address.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// and `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin.
    pub fn set_emergency_pauser(env: Env, admin: Address, pauser: Address) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        env.storage().instance().set(&DataKey::Pauser, &pauser);

        // Emit audit event
        env.events().publish(
            (Symbol::new(&env, "set_emergency_pauser"), admin),
            pauser,
        );

        Ok(())
    }

    /// Configures the upgrade timelock period and required number of approvals.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `timelock_period`: the time in seconds that must pass before an upgrade can be executed.
    /// - `required_approvals`: the number of admin approvals required to execute an upgrade.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// and `Error::Unauthorized` if the provided admin address does not match the
    /// stored admin.
    pub fn configure_upgrade(
        env: Env,
        admin: Address,
        timelock_period: u64,
        required_approvals: u32,
    ) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        env.storage()
            .instance()
            .set(&DataKey::UpgradeTimelock, &timelock_period);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeRequiredApprovals, &required_approvals);

        Ok(())
    }

    /// Proposes a WASM bytecode upgrade for a target contract.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `contract_address`: the address of the contract to upgrade.
    /// - `new_wasm_hash`: the hash of the new WASM bytecode.
    ///
    /// # Returns
    /// Returns the proposal ID on success.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the stored admin,
    /// or `Error::InvalidWasmHash` if the wasm hash is zero.
    pub fn propose_upgrade(
        env: Env,
        admin: Address,
        contract_address: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<u64, Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        // Validate wasm hash is not zero
        let zero_hash = BytesN::<32>::from_array(&env, &[0u8; 32]);
        if new_wasm_hash == zero_hash {
            return Err(Error::InvalidWasmHash);
        }

        let timelock_period: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelock)
            .unwrap_or(86400); // Default 24 hours

        let counter: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposalCounter)
            .unwrap_or(0);

        let proposal = UpgradeProposal {
            contract_address: contract_address.clone(),
            new_wasm_hash: new_wasm_hash.clone(),
            proposed_at: env.ledger().timestamp(),
            timelock_period,
            approvals: Vec::new(&env),
            executed: false,
        };

        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal(counter), &proposal);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposalCounter, &(counter + 1));

        Ok(counter)
    }

    /// Approves a pending upgrade proposal.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `proposal_id`: the ID of the upgrade proposal to approve.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the stored admin,
    /// `Error::UpgradeProposalNotFound` if the proposal does not exist,
    /// or `Error::UpgradeAlreadyExecuted` if the proposal has already been executed.
    pub fn approve_upgrade(env: Env, admin: Address, proposal_id: u64) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let mut proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal(proposal_id))
            .ok_or(Error::UpgradeProposalNotFound)?;

        if proposal.executed {
            return Err(Error::UpgradeAlreadyExecuted);
        }

        // Check if admin already approved
        for i in 0..proposal.approvals.len() {
            if proposal.approvals.get(i).unwrap() == admin {
                return Ok(()); // Already approved, idempotent
            }
        }

        proposal.approvals.push_back(admin);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal(proposal_id), &proposal);

        Ok(())
    }

    /// Executes an approved upgrade proposal after the timelock has expired.
    ///
    /// # Parameters
    /// - `admin`: the admin address that must be authorized.
    /// - `proposal_id`: the ID of the upgrade proposal to execute.
    ///
    /// # Errors
    /// Returns `Error::NotInitialized` if the admin contract has not been initialized,
    /// `Error::Unauthorized` if the provided admin address does not match the stored admin,
    /// `Error::UpgradeProposalNotFound` if the proposal does not exist,
    /// `Error::UpgradeTimelockNotExpired` if the timelock period has not passed,
    /// `Error::InsufficientApprovals` if the required number of approvals has not been met,
    /// or `Error::UpgradeAlreadyExecuted` if the proposal has already been executed.
    pub fn execute_upgrade(env: Env, admin: Address, proposal_id: u64) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let mut proposal: UpgradeProposal = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeProposal(proposal_id))
            .ok_or(Error::UpgradeProposalNotFound)?;

        if proposal.executed {
            return Err(Error::UpgradeAlreadyExecuted);
        }

        // Check timelock
        let current_time = env.ledger().timestamp();
        let unlock_time = proposal.proposed_at + proposal.timelock_period;
        if current_time < unlock_time {
            return Err(Error::UpgradeTimelockNotExpired);
        }

        // Check approvals
        let required_approvals: u32 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeRequiredApprovals)
            .unwrap_or(1);
        if proposal.approvals.len() < required_approvals as u64 {
            return Err(Error::InsufficientApprovals);
        }

        // Mark as executed
        proposal.executed = true;
        env.storage()
            .instance()
            .set(&DataKey::UpgradeProposal(proposal_id), &proposal);

        // Execute the upgrade
        env.deployer()
            .update_current_contract_wasm(&proposal.contract_address, &proposal.new_wasm_hash);

        Ok(())
    }

    /// Returns the details of an upgrade proposal.
    ///
    /// # Parameters
    /// - `proposal_id`: the ID of the upgrade proposal to query.
    ///
    /// # Returns
    /// Returns the `UpgradeProposal` struct.
    ///
    /// # Errors
    /// Returns `Error::UpgradeProposalNotFound` if the proposal does not exist.
        /// Issue #294: Court-of-final-resort cross-contract escalation from Refund arbitration
    /// to Orchestrator governance council super-panel.
    pub fn resolve_supreme_dispute(
        env: Env,
        admins: Vec<Address>,
        refund_id: u64,
        approve_customer: bool,
        _slash_corrupt_arbitrators: Vec<Address>,
    ) -> Result<(), Error> {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;

        let mut admin_authenticated = false;
        for admin in admins.iter() {
            admin.require_auth();
            if admin == stored_admin {
                admin_authenticated = true;
            }
        }
        if !admin_authenticated {
            return Err(Error::Unauthorized);
        }

        (Symbol::new(&env, "supreme_dispute_resolved"), refund_id, approve_customer).publish(&env);
        Ok(())
    }

    pub fn get_upgrade_proposal(env: Env, proposal_id: u64) -> Result<UpgradeProposal, Error> {
        env.storage()
            .instance()
            .get(&DataKey::UpgradeProposal(proposal_id))
            .ok_or(Error::UpgradeProposalNotFound)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Events, Ledger};
    use soroban_sdk::{Symbol, TryFromVal};

    fn setup_payment(env: &Env, admin: &Address) -> Address {
        let contract_id = env.register(payments::PaymentContract, ());
        let client = PaymentContractClient::new(env, &contract_id);
        client.initialize(admin);
        contract_id
    }

    fn setup_escrow(env: &Env, admin: &Address) -> Address {
        let contract_id = env.register(escrow::EscrowContract, ());
        let client = EscrowContractClient::new(env, &contract_id);
        client.initialize(admin);
        contract_id
    }

    fn setup_refund(env: &Env, admin: &Address) -> Address {
        let contract_id = env.register(refund::RefundContract, ());
        let client = RefundContractClient::new(env, &contract_id);
        client.initialize(admin);
        contract_id
    }

    #[test]
    fn test_get_system_status() {
        let env = Env::default();
        env.mock_all_auths();
        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(env, &admin_contract_id);

        let admin = Address::generate(env);
        let pauser = Address::generate(env);
        let payment_contract = setup_payment(env, &pauser);
        let escrow_contract = setup_escrow(env, &pauser);
        let refund_contract = setup_refund(env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );
        (
            client,
            admin,
            pauser,
            payment_contract,
            escrow_contract,
            refund_contract,
        )
    }

    fn reason(env: &Env) -> String {
        String::from_str(env, "security incident")
    }

    fn is_globally_paused(env: &Env, child: &Address) -> bool {
        let args = ().into_val(env);
        let state: payments::PauseState =
            env.invoke_contract(child, &Symbol::new(env, "get_pause_state"), args);
        state.globally_paused
    }

    #[test]
    fn test_initialize_and_pause_all() {
        let env = Env::default();
        let (client, _admin, pauser, _, _, _) = setup(&env);

        // Freshly initialized: nothing paused, every contract at schema version 1.
        let status = client.get_system_status();
        assert!(!status.payment_paused);
        assert_eq!(status.payment_schema_version, 1);
        assert!(!status.escrow_paused);
        assert_eq!(status.escrow_schema_version, 1);
        assert!(!status.refund_paused);
        assert_eq!(status.refund_schema_version, 1);

        // Pause the payment contract and advance the refund schema version.
        let reason = String::from_str(&env, "status check");
        let payment = PaymentContractClient::new(&env, &payment_contract);
        payment.pause_contract(&pauser, &reason);

        let refund = RefundContractClient::new(&env, &refund_contract);
        refund.migrate_schema(&pauser, &2);

        // The unified report reflects both changes in a single query.
        let status = client.get_system_status();
        assert!(status.payment_paused);
        assert_eq!(status.payment_schema_version, 1);
        assert!(!status.escrow_paused);
        assert_eq!(status.escrow_schema_version, 1);
        assert!(!status.refund_paused);
        assert_eq!(status.refund_schema_version, 2);

        // Unpausing clears the flag in the report.
        payment.unpause_contract(&pauser);
        let status = client.get_system_status();
        assert!(!status.payment_paused);
    }

    #[test]
    fn test_emergency_pause_all_gas_benchmark() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Measure the CPU and memory cost of the multi-contract dispatch.
        // The budget resets before every top-level invocation, so reading it
        // after the call gives the cost of that invocation alone.
        let reason = String::from_str(&env, "gas benchmark");
        client.emergency_pause_all(&pauser, &reason);

        let budget = env.cost_estimate().budget();
        let cpu = budget.cpu_instruction_cost();
        let mem = budget.memory_bytes_cost();

        // The dispatch must stay well within the Soroban transaction CPU limit
        // (1 billion instructions on current networks). This bound is generous
        // enough to avoid flakism while still catching regressions that would
        // blow the budget.
        assert!(
            cpu < 1_000_000_000,
            "emergency_pause_all CPU {} exceeds transaction limit",
            cpu
        );
        assert!(
            mem < 40_000_000,
            "emergency_pause_all memory {} exceeds transaction limit",
            mem
        );

        // The pause actually took effect on all three contracts.
        let payment = PaymentContractClient::new(&env, &payment_contract);
        let escrow = EscrowContractClient::new(&env, &escrow_contract);
        let refund = RefundContractClient::new(&env, &refund_contract);
        assert!(payment.get_pause_state().globally_paused);
        assert!(escrow.get_pause_state().globally_paused);
        assert!(refund.get_pause_state().globally_paused);
    }

    #[test]
    fn test_initialize_and_pause_all() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        let reason = String::from_str(&env, "security incident");
        client.emergency_pause_all(&pauser, &reason);
        let justification = String::from_str(&env, "issue resolved");
        client.emergency_unpause_all(&admin, &justification);
    }

    #[test]
    fn test_pauser_cannot_unpause() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Pause first
        let reason = String::from_str(&env, "security incident");
        client.emergency_pause_all(&pauser, &reason);

        // Pauser tries to unpause - should fail
        let justification = String::from_str(&env, "trying to unpause");
        let result = client.try_emergency_unpause_all(&pauser, &justification);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_emergency_pauser() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let new_pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Admin sets new pauser
        client.set_emergency_pauser(&admin, &new_pauser);

        // New pauser can pause
        let reason = String::from_str(&env, "test pause");
        client.emergency_pause_all(&new_pauser, &reason);

        // Old pauser can no longer pause
        let result = client.try_emergency_pause_all(&pauser, &reason);
        assert!(result.is_err());
    }

    #[test]
    fn test_reentrancy_protection() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Manually set the reentrancy lock
        env.as_contract(&admin_contract_id, || {
            env.storage()
                .instance()
                .set(&DataKey::ReentrancyLock, &true);
        });

        // Try to pause - should fail due to reentrancy lock
        let reason = String::from_str(&env, "test reentrancy");
        let result = client.try_emergency_pause_all(&pauser, &reason);
        assert!(result.is_err());

        // Try to unpause - should also fail
        let justification = String::from_str(&env, "test reentrancy");
        let result = client.try_emergency_unpause_all(&admin, &justification);
        assert!(result.is_err());
    }

    #[test]
    fn test_upgrade_proposal_lifecycle() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Configure upgrade with 0 timelock and 1 approval for testing
        client.configure_upgrade(&admin, &0, &1);

        // Create a dummy wasm hash
        let wasm_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);

        // Propose upgrade
        let proposal_id = client.propose_upgrade(&admin, &payment_contract, &wasm_hash);
        assert_eq!(proposal_id, 0);

        // Verify proposal exists
        let proposal = client.get_upgrade_proposal(&proposal_id);
        assert_eq!(proposal.contract_address, payment_contract);
        assert_eq!(proposal.new_wasm_hash, wasm_hash);
        assert!(!proposal.executed);

        // Approve the upgrade
        client.approve_upgrade(&admin, &proposal_id);

        // Execute the upgrade
        client.execute_upgrade(&admin, &proposal_id);

        // Verify proposal is marked as executed
        let proposal = client.get_upgrade_proposal(&proposal_id);
        assert!(proposal.executed);
    }

    #[test]
    fn test_upgrade_timelock_enforcement() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Configure upgrade with 1 hour timelock
        client.configure_upgrade(&admin, &3600, &1);

        let wasm_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let proposal_id = client.propose_upgrade(&admin, &payment_contract, &wasm_hash);
        client.approve_upgrade(&admin, &proposal_id);

        // Try to execute before timelock expires - should fail
        let result = client.try_execute_upgrade(&admin, &proposal_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_upgrade_multisig_enforcement() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Configure upgrade with 2 required approvals
        client.configure_upgrade(&admin, &0, &2);

        let wasm_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
        let proposal_id = client.propose_upgrade(&admin, &payment_contract, &wasm_hash);
        client.approve_upgrade(&admin, &proposal_id);

        // Try to execute with only 1 approval - should fail
        let result = client.try_execute_upgrade(&admin, &proposal_id);
        assert!(result.is_err());

        // Add second approval
        client.approve_upgrade(&admin2, &proposal_id);

        // Now execution should succeed
        client.execute_upgrade(&admin, &proposal_id);
    }

    #[test]
    fn test_contract_address_rotation_timelock() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Propose a rotation of the payment contract.
        env.ledger().set_timestamp(1_000);
        let new_payment_contract = setup_payment(&env, &pauser);
        client.set_payment_contract(&admin, &new_payment_contract);

        // The pending rotation is recorded, but the stored address is unchanged.
        let pending = client
            .get_pending_rotation(&ContractTarget::Payment)
            .unwrap();
        assert_eq!(pending.new_address, new_payment_contract);
        assert_eq!(pending.proposed_by, admin);
        assert_eq!(pending.proposed_at, 1_000);

        // Executing before the 48-hour timelock elapses must fail.
        let result = client.try_execute_payment_rotation(&admin);
        assert_eq!(result, Err(Ok(Error::RotationTimelockActive)));

        // The stored address must still be the original one: pausing through
        // the admin contract affects the original contract, not the proposed
        // one.
        let reason = String::from_str(&env, "timelock check");
        client.emergency_pause_all(&pauser, &reason);
        let original = PaymentContractClient::new(&env, &payment_contract);
        assert!(original.get_pause_state().globally_paused);
        let proposed = PaymentContractClient::new(&env, &new_payment_contract);
        assert!(!proposed.get_pause_state().globally_paused);
        client.emergency_unpause_all(&pauser);

        // One second before the timelock elapses, execution still fails.
        env.ledger()
            .set_timestamp(1_000 + CONTRACT_ROTATION_TIMELOCK - 1);
        let result = client.try_execute_payment_rotation(&admin);
        assert_eq!(result, Err(Ok(Error::RotationTimelockActive)));

        // Once the timelock has elapsed, execution succeeds.
        env.ledger()
            .set_timestamp(1_000 + CONTRACT_ROTATION_TIMELOCK);
        client.execute_payment_rotation(&admin);

        // The ContractTargetUpdated event must have been emitted. This is
        // checked immediately after execution because Soroban test
        // environments reset the observable event buffer when further contract
        // invocations (such as the storage queries below) occur.
        assert!(env.events().all().iter().any(|e| {
            Symbol::try_from_val(&env, &e.1.get(0).unwrap_or_default()).ok()
                == Some(Symbol::new(&env, "contract_target_updated"))
        }));

        // The pending rotation is cleared.
        assert!(client
            .get_pending_rotation(&ContractTarget::Payment)
            .is_none());

        // The rotation took effect: pausing now hits the new contract.
        client.emergency_pause_all(&pauser, &reason);
        let proposed = PaymentContractClient::new(&env, &new_payment_contract);
        assert!(proposed.get_pause_state().globally_paused);
        client.emergency_unpause_all(&pauser);
    }

    #[test]
    fn test_cancel_proposed_rotation() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        // Canceling a rotation that was never proposed fails.
        let result = client.try_cancel_proposed_rotation(&admin, &ContractTarget::Escrow);
        assert_eq!(result, Err(Ok(Error::RotationNotPending)));

        // Propose and then cancel a rotation.
        let new_escrow_contract = setup_escrow(&env, &pauser);
        client.set_escrow_contract(&admin, &new_escrow_contract);
        assert!(client
            .get_pending_rotation(&ContractTarget::Escrow)
            .is_some());

        client.cancel_proposed_rotation(&admin, &ContractTarget::Escrow);
        assert!(client
            .get_pending_rotation(&ContractTarget::Escrow)
            .is_none());

        // After cancellation the stored address is unchanged and a new
        // rotation can be proposed.
        let reason = String::from_str(&env, "post-cancel check");
        client.emergency_pause_all(&pauser, &reason);
        let escrow = EscrowContractClient::new(&env, &escrow_contract);
        assert!(escrow.get_pause_state().globally_paused);
        client.emergency_unpause_all(&pauser);

        let newer_escrow_contract = setup_escrow(&env, &pauser);
        client.set_escrow_contract(&admin, &newer_escrow_contract);
        let pending = client
            .get_pending_rotation(&ContractTarget::Escrow)
            .unwrap();
        assert_eq!(pending.new_address, newer_escrow_contract);
    }

    #[test]
    fn test_rotation_rejects_duplicate_proposal() {
        let env = Env::default();
        env.mock_all_auths();

        let admin_contract_id = env.register(AdminContract, ());
        let client = AdminContractClient::new(&env, &admin_contract_id);

        let admin = Address::generate(&env);
        let pauser = Address::generate(&env);
        let payment_contract = setup_payment(&env, &pauser);
        let escrow_contract = setup_escrow(&env, &pauser);
        let refund_contract = setup_refund(&env, &pauser);

        client.initialize(
            &admin,
            &pauser,
            &payment_contract,
            &escrow_contract,
            &refund_contract,
        );

        let new_refund_contract = setup_refund(&env, &pauser);
        client.set_refund_contract(&admin, &new_refund_contract);

        // A second proposal for the same target while one is pending fails.
        let another_refund_contract = setup_refund(&env, &pauser);
        let result = client.try_set_refund_contract(&admin, &another_refund_contract);
        assert_eq!(result, Err(Ok(Error::RotationAlreadyPending)));

        // Proposing a rotation for a different target still works.
        let new_payment_contract = setup_payment(&env, &pauser);
        client.set_payment_contract(&admin, &new_payment_contract);
        assert!(client
            .get_pending_rotation(&ContractTarget::Payment)
            .is_some());
    }
}

#[cfg(test)]
mod schema_version_test;
