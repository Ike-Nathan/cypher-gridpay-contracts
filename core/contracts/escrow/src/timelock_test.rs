#[cfg(test)]
mod timelock_tests {
    use crate::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token, Address, Bytes, Env,
    };

    fn setup_test(env: &Env) -> (EscrowContractClient, Address) {
        env.mock_all_auths();

        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);
        (client, admin)
    }

    fn setup_vesting(env: &Env, amount: i128) -> (EscrowContractClient, Address, Address, Address) {
        env.mock_all_auths();
        env.ledger().set_timestamp(100);

        let contract_id = env.register(EscrowContract, ());
        let client = EscrowContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(&admin);

        let grantor = Address::generate(env);
        let beneficiary = Address::generate(env);
        let token_address = env.register_stellar_asset_contract(grantor.clone());
        let token_admin = token::StellarAssetClient::new(env, &token_address);
        token_admin.mint(&contract_id, &amount);

        let escrow_id = client.create_vesting_escrow(
            &grantor,
            &beneficiary,
            &amount,
            &token_address,
            &110,
            &200,
            &Vec::new(env),
        );
        client.set_vesting_curve_config(&VestingAccelerationConfig {
            schedule_id: escrow_id,
            milestone_bps: 0,
            max_acceleration_bps: 10_000,
            total_accelerated_bps: 0,
            start_ts: 100,
            cliff_duration_secs: 10,
            cliff_unlock_bps: 2_000,
            linear_duration_secs: 90,
            single_trigger_accel_bps: 2_500,
            double_trigger_accel_bps: 7_500,
            trigger_one_confirmed: false,
            trigger_two_confirmed: false,
        });

        (client, grantor, beneficiary, token_address)
    }

    #[test]
    fn test_queue_action() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let escrow_id = 1u64;
        let action_type = EscrowActionType::ResolveDispute(true);
        let data = Bytes::new(&env);

        // We need to create an escrow first so it exists in storage
        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        client.create_escrow(&customer, &merchant, &1000_i128, &token, &5000_u64, &0_u64);

        let action_id = client.queue_action(&admin, &escrow_id, &action_type, &data);

        assert_eq!(action_id, 1);

        let queued_action = client.get_queued_action(&action_id);
        assert_eq!(queued_action.escrow_id, escrow_id);
        assert!(!queued_action.executed);
        assert!(!queued_action.cancelled);
    }

    #[test]
    fn test_execute_action_too_early() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let escrow_id = 1u64;
        let action_type = EscrowActionType::ResolveDispute(true);
        let data = Bytes::new(&env);

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        client.create_escrow(&customer, &merchant, &1000_i128, &token, &5000_u64, &0_u64);
        client.dispute_escrow(&customer, &escrow_id);

        let action_id = client.queue_action(&admin, &escrow_id, &action_type, &data);

        // Try to execute immediately - should fail with TimelockNotElapsed since the
        // security delay (timelock) has not yet elapsed
        let result = client.try_execute_queued_action(&action_id);
        assert_eq!(
            result,
            Err(Ok(Error::Escrow(EscrowError::TimelockNotElapsed)))
        );
    }

    #[test]
    fn test_cancel_queued_action() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let escrow_id = 1u64;
        let action_type = EscrowActionType::ForceRelease;
        let data = Bytes::new(&env);

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        client.create_escrow(&customer, &merchant, &1000_i128, &token, &5000_u64, &0_u64);

        let action_id = client.queue_action(&admin, &escrow_id, &action_type, &data);

        client.cancel_queued_action(&admin, &action_id);

        let queued_action = client.get_queued_action(&action_id);
        assert!(queued_action.cancelled);
    }

    #[test]
    fn test_set_timelock_config() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        let config = TimeLockConfig {
            delay: 7200,        // 2 hours
            grace_period: 3600, // 1 hour
        };

        client.set_timelock_config(&admin, &config);

        let stored_config = client.get_timelock_config();
        assert_eq!(stored_config.delay, config.delay);
        assert_eq!(stored_config.grace_period, config.grace_period);
    }

    #[test]
    fn test_invalid_timelock_delay() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup_test(&env);

        // Test delay too short (less than 1 hour)

        let config = TimeLockConfig {
            delay: 0,
            grace_period: 3600,
        };

        let result = client.try_set_timelock_config(&admin, &config);
        assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
        // Test delay too long (more than 7 days)
        let config = TimeLockConfig {
            delay: 700000, // > 7 days
            grace_period: 3600,
        };

        let result = client.try_set_timelock_config(&admin, &config);
        assert_eq!(result, Err(Ok(Error::Escrow(EscrowError::InvalidStatus))));
    }

    #[test]
    fn test_vesting_cliff_boundary() {
        let env = Env::default();
        let (client, _, _, _) = setup_vesting(&env, 10_000);

        assert_eq!(client.compute_vested_amount(&1, &109), 0);
        assert_eq!(client.compute_vested_amount(&1, &110), 2_000);
    }

    #[test]
    fn test_vesting_linear_interpolation() {
        let env = Env::default();
        let (client, _, _, _) = setup_vesting(&env, 10_000);

        assert_eq!(client.compute_vested_amount(&1, &155), 6_000);
    }

    #[test]
    fn test_double_trigger_acceleration_requires_both_parties() {
        let env = Env::default();
        let (client, grantor, beneficiary, _) = setup_vesting(&env, 10_000);

        client.confirm_vesting_trigger(&1, &1, &grantor);
        assert_eq!(client.compute_vested_amount(&1, &110), 4_000);
        client.confirm_vesting_trigger(&1, &2, &beneficiary);
        assert_eq!(client.compute_vested_amount(&1, &110), 8_000);
    }

    #[test]
    fn test_mid_stream_revocation_splits_vested_and_unvested_balances() {
        let env = Env::default();
        let (client, grantor, beneficiary, token_address) = setup_vesting(&env, 10_000);
        let token_client = token::Client::new(&env, &token_address);
        let contract_address = client.address.clone();

        env.ledger().set_timestamp(110);
        assert_eq!(client.claim_vested_escrow_tokens(&1, &beneficiary), 2_000);
        env.ledger().set_timestamp(155);
        client.revoke_unvested_escrow(&1, &grantor);

        assert_eq!(token_client.balance(&beneficiary), 6_000);
        assert_eq!(token_client.balance(&grantor), 4_000);
        assert_eq!(token_client.balance(&contract_address), 0);
        assert_eq!(
            client.try_claim_vested_escrow_tokens(&1, &beneficiary),
            Err(Ok(Error::Escrow(EscrowError::AlreadyProcessed)))
        );
    }
}
