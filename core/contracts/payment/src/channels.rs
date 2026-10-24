// Payment channel logic and operations
// TODO: Extract payment channel-related functions from lib.rs

//! Canonical signing domain for off-chain signatures.
//!
//! Every message a party signs off-chain for this contract is bound to the
//! network, the contract instance and the entry point it is meant for, so a
//! signature cannot be replayed on another network (Testnet -> Mainnet), on
//! another deployment of the contract, or against another entry point.
//!
//! Byte layout (documented for integrators in SECURITY.md and
//! sdk/typescript/README.md):
//!
//! ```text
//! domain_hash = sha256(
//!     "CYPHER_GRIDPAY_DOMAIN_V1"          24 bytes, ASCII
//!  || network_id                         32 bytes, sha256(network passphrase)
//!  || contract_address                   XDR ScVal of the contract address (40 bytes)
//!  || action_tag                         ASCII entry-point tag, e.g. "channel_settle"
//! )
//! digest = sha256(0x19 || 0x01 || domain_hash || struct_hash)      66-byte pre-image
//! ```
//!
//! The signer signs the 32-byte `digest`. `struct_hash` is defined per action;
//! see [`channel_settle_struct_hash`].
use soroban_sdk::{contracttype, xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol};

/// Domain tag; the trailing `V1` is the domain version.
pub const DOMAIN_TAG: &[u8] = b"CYPHER_GRIDPAY_DOMAIN_V1";
/// Version encoded in [`DOMAIN_TAG`].
pub const DOMAIN_VERSION: u32 = 1;
/// Prefix of the signed pre-image, so a digest can never be mistaken for a
/// plain hash or another encoding.
pub const SIGNED_DIGEST_PREFIX: [u8; 2] = [0x19, 0x01];
/// Action tag for [`crate::PaymentContract::settle_channel`].
pub const ACTION_CHANNEL_SETTLE: &str = "channel_settle";

/// The domain a signature is bound to. `protocol_name` and `version` are
/// informational (they are fixed by [`DOMAIN_TAG`]); `network_id` and
/// `contract_address` are the values hashed into every digest.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainSeparator {
    pub protocol_name: Symbol,
    pub version: u32,
    pub network_id: BytesN<32>,
    pub contract_address: Address,
}

impl DomainSeparator {
    /// The domain of the running contract on the current network.
    pub fn current(env: &Env) -> Self {
        DomainSeparator {
            protocol_name: Symbol::new(env, "CYPHER_GRIDPAY"),
            version: DOMAIN_VERSION,
            network_id: env.ledger().network_id(),
            contract_address: env.current_contract_address(),
        }
    }

    /// `domain_hash` for `action` (see the module docs for the layout).
    pub fn hash(&self, env: &Env, action: &str) -> BytesN<32> {
        let mut pre = Bytes::from_slice(env, DOMAIN_TAG);
        pre.append(&Bytes::from(self.network_id.clone()));
        pre.append(&self.contract_address.clone().to_xdr(env));
        pre.append(&Bytes::from_slice(env, action.as_bytes()));
        env.crypto().sha256(&pre).to_bytes()
    }
}

/// The digest a signer must sign for `action` with the given `struct_hash`:
/// `sha256(0x1901 || domain_hash || struct_hash)`.
pub fn signing_digest(env: &Env, action: &str, struct_hash: &BytesN<32>) -> BytesN<32> {
    let domain_hash = DomainSeparator::current(env).hash(env, action);
    let mut pre = Bytes::from_slice(env, &SIGNED_DIGEST_PREFIX);
    pre.append(&Bytes::from(domain_hash));
    pre.append(&Bytes::from(struct_hash.clone()));
    env.crypto().sha256(&pre).to_bytes()
}

/// `struct_hash` of a channel settlement:
/// `sha256(xdr(channel_id: u64) || xdr(merchant_amount: i128) || xdr(nonce: u64))`.
pub fn channel_settle_struct_hash(
    env: &Env,
    channel_id: u64,
    merchant_amount: i128,
    nonce: u64,
) -> BytesN<32> {
    let mut pre = Bytes::new(env);
    pre.append(&channel_id.to_xdr(env));
    pre.append(&merchant_amount.to_xdr(env));
    pre.append(&nonce.to_xdr(env));
    env.crypto().sha256(&pre).to_bytes()
}

/// Verifies `signature` by `public_key` over the domain-bound digest of
/// `struct_hash` for `action`. The domain is always this contract on this
/// network, so a signature made for another contract, network or action
/// fails. Traps on an invalid signature, like `ed25519_verify`.
pub fn verify_domain_signature(
    env: &Env,
    public_key: &BytesN<32>,
    action: &str,
    struct_hash: &BytesN<32>,
    signature: &BytesN<64>,
) {
    let digest = signing_digest(env, action, struct_hash);
    env.crypto()
        .ed25519_verify(public_key, &Bytes::from(digest), signature);
}
