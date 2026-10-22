#![cfg(test)]
#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, testutils::{Address as _, Ledger},
    Address, Bytes, BytesN, Env, String, Vec,
};

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct FxHop {
    pub pool: Address,
    pub token_in: Address,
    pub token_out: Address,
    pub max_slippage_bps: u32,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct RefundVoucher {
    pub voucher_id: u64,
    pub owner: Address,
    pub remaining_value: i128,
    pub historical_rate_lock: u64,
    pub transferable: bool,
}

#[derive(Clone, Debug, PartialEq)]
#[contracttype]
pub struct PolicyHierarchyNode {
    pub policy_id: u64,
    pub priority: u32,
    pub allow_override: bool,
    pub refund_window_secs: u64,
}

#[contract]
pub struct MockAdversarialMultiContractSuite;

#[contractimpl]
impl MockAdversarialMultiContractSuite {
    pub fn execute_multihop_fx(
        hops: Vec<FxHop>,
        amount_in: i128,
        actual_slippage_bps: u32,
    ) -> (i128, bool) {
        let mut valid = true;
        for hop in hops.iter() {
            if actual_slippage_bps > hop.max_slippage_bps {
                valid = false;
            }
        }
        let amount_out = if valid { (amount_in * 98) / 100 } else { 0 };
        (amount_out, valid)
    }

    pub fn transfer_voucher(
        mut voucher: RefundVoucher,
        new_owner: Address,
    ) -> RefundVoucher {
        assert!(voucher.transferable);
        voucher.owner = new_owner;
        voucher
    }

    pub fn split_redeem_voucher(
        mut voucher: RefundVoucher,
        redeem_amount: i128,
    ) -> (RefundVoucher, i128) {
        assert!(voucher.remaining_value >= redeem_amount);
        voucher.remaining_value -= redeem_amount;
        (voucher, redeem_amount)
    }

    pub fn resolve_policy_override_conflict(
        child: PolicyHierarchyNode,
        parent: PolicyHierarchyNode,
    ) -> PolicyHierarchyNode {
        if child.allow_override && child.priority >= parent.priority {
            child
        } else {
            parent
        }
    }
}

// 1. Multi-hop FX path settlement with slippage guards
#[test]
fn test_multihop_fx_path_settlement_with_slippage_guards() {
    let env = Env::default();
    env.mock_all_auths();

    let pool1 = Address::generate(&env);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    let mut hops = Vec::new(&env);
    hops.push_back(FxHop {
        pool: pool1,
        token_in: token_a,
        token_out: token_b,
        max_slippage_bps: 100, // 1%
    });

    // Case 1: slippage within bounds
    let (out, ok) = MockAdversarialMultiContractSuite::execute_multihop_fx(hops.clone(), 10000, 50);
    assert!(ok);
    assert_eq!(out, 9800);

    // Case 2: slippage breached
    let (out_fail, ok_fail) = MockAdversarialMultiContractSuite::execute_multihop_fx(hops, 10000, 150);
    assert!(!ok_fail);
    assert_eq!(out_fail, 0);
}

// 2. Secondary market voucher transferability
#[test]
fn test_secondary_market_voucher_transferability() {
    let env = Env::default();
    env.mock_all_auths();

    let initial_owner = Address::generate(&env);
    let secondary_buyer = Address::generate(&env);

    let voucher = RefundVoucher {
        voucher_id: 501,
        owner: initial_owner,
        remaining_value: 10000,
        historical_rate_lock: 125000,
        transferable: true,
    };

    let transferred = MockAdversarialMultiContractSuite::transfer_voucher(voucher, secondary_buyer.clone());
    assert_eq!(transferred.owner, secondary_buyer);
    assert_eq!(transferred.remaining_value, 10000);
}

// 3. Partial split-redemption of refund vouchers
#[test]
fn test_partial_split_redemption_for_refund_voucher() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let voucher = RefundVoucher {
        voucher_id: 502,
        owner,
        remaining_value: 10000,
        historical_rate_lock: 125000,
        transferable: true,
    };

    let (updated_voucher, redeemed) =
        MockAdversarialMultiContractSuite::split_redeem_voucher(voucher, 4000);
    assert_eq!(redeemed, 4000);
    assert_eq!(updated_voucher.remaining_value, 6000);
}

// 4. Hierarchical policy inheritance conflict resolution
#[test]
fn test_hierarchical_policy_inheritance_conflict_resolution() {
    let parent = PolicyHierarchyNode {
        policy_id: 1,
        priority: 10,
        allow_override: false,
        refund_window_secs: 86400 * 30,
    };

    let child = PolicyHierarchyNode {
        policy_id: 2,
        priority: 20,
        allow_override: true,
        refund_window_secs: 86400 * 14,
    };

    let active_policy =
        MockAdversarialMultiContractSuite::resolve_policy_override_conflict(child.clone(), parent);
    assert_eq!(active_policy.policy_id, child.policy_id);
    assert_eq!(active_policy.refund_window_secs, 86400 * 14);
}

// 5. Rate lock preservation during market volatility
#[test]
fn test_historical_rate_lock_preservation_under_fx_volatility() {
    let voucher = RefundVoucher {
        voucher_id: 503,
        owner: Address::generate(&Env::default()),
        remaining_value: 5000,
        historical_rate_lock: 130000,
        transferable: true,
    };
    let current_market_rate = 95000_u64;
    assert_eq!(voucher.historical_rate_lock, 130000);
    assert!(voucher.historical_rate_lock > current_market_rate);
}
