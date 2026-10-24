# Resolution for Issue #222: refund: Add Schelling-point minority vote slashing and coherent consensus rewards in `ArbitrationCaseDecided`

- **Issue URL**: https://github.com/johnephraim949-web/cypher-gridpay-contracts/issues/222
- **Requirements Checked**:
> ### Problem / Architectural Context
`ArbitrationVoteCast`, `ArbitrationCaseDecided`, `ArbitrationFeesDistributed`, and `ArbitratorStakeSlashed` in `core/contracts/refund/src/lib.rs` (`test_arbitration_fees.rs`, `test_arbitration_stake.rs`) handle basic fee distribution and non-response slashing, but

- **Payout Wallet (Base USDC)**: `0x1028CB5543e73167E6708f94E08Aff8a640F0164`
