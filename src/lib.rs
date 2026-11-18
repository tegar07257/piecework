// PieceWork — milestone escrow for freelance work on Tari Ootle.
//
// A client (payer) creates an agreement naming a payee and a list of milestone
// amounts. The client funds the agreement in TARI. For each milestone, the
// payee marks it delivered and the client approves it, which releases exactly
// that milestone's slice from the escrow vault. Funds that were never released
// become refundable to the client once the agreement deadline (an epoch) has
// passed. Either party can cancel the agreement while it is still unfunded.
//
// Authorization model: the component's method access rules are open, and every
// state-changing method checks the transaction signer's public key against the
// stored payer/payee keys itself. Creating the agreement records the signer as
// the payer. Only the payer can fund, approve and refund; only the payee can
// mark milestones delivered; cancel is open to both parties. This keeps the
// rules visible in one place and directly covered by the engine tests.

use tari_template_lib::prelude::*;

#[template]
mod piecework {
    use super::*;

    pub struct PieceWork {
        /// Public key of the client who created (and funds) the agreement.
        payer: RistrettoPublicKeyBytes,
        /// Public key of the freelancer who delivers the work.
        payee: RistrettoPublicKeyBytes,
        /// Ordered milestones; each is delivered and released independently.
        milestones: Vec<Milestone>,
        /// Escrow vault holding the funded TARI until release or refund.
        vault: Vault,
        /// Cumulative amount ever deposited (µT).
        funded_total: u64,
        /// Cumulative amount released to the payee (µT).
        released_total: u64,
        /// Epoch at/after which unreleased funds become refundable.
        deadline_epoch: u64,
        /// True once either party cancels an unfunded agreement (terminal).
        cancelled: bool,
        /// True once the client refunds the unreleased remainder (terminal).
        closed: bool,
    }

    impl PieceWork {
        /// Create a new agreement. The transaction signer becomes the payer.
        ///
        /// * `payee` — public key of the freelancer.
        /// * `milestones` — milestone amounts in µT; must be non-empty, all > 0.
        /// * `deadline_epoch` — refund deadline; must be in the future.
        pub fn new(
            payee: RistrettoPublicKeyBytes,
            milestones: Vec<u64>,
            deadline_epoch: u64,
        ) -> Component<Self> {
            let payer = CallerContext::transaction_signer_public_key();
            assert!(payee != payer, "payee must differ from the payer (the creator)");
            assert!(!milestones.is_empty(), "at least one milestone is required");
            assert!(
                milestones.iter().all(|m| *m > 0),
                "every milestone amount must be greater than zero"
            );
            assert!(
                deadline_epoch > Consensus::current_epoch(),
                "deadline_epoch must be in the future"
            );

            let state = Self {
                payer,
                payee,
                milestones: milestones
                    .into_iter()
                    .map(|amount| Milestone { amount, delivered: false, released: false })
                    .collect(),
                vault: Vault::new_empty(TARI_TOKEN),
                funded_total: 0,
                released_total: 0,
                deadline_epoch,
                cancelled: false,
                closed: false,
            };

            // Method-level signer checks inside each method are the
            // authorization layer, so the component rules allow all calls and
            // are locked (no owner can rewrite them later).
            Component::new(state)
                .with_access_rules(AccessRules::allow_all())
                .with_owner_rule(OwnerRule::None)
                .create()
        }

        /// Fund the agreement. Only the payer may fund, and total funding may
        /// not exceed the sum of the milestone amounts.
        pub fn fund(&mut self, bucket: Bucket) {
            self.assert_open();
            self.assert_payer();
            assert!(
                bucket.resource_address() == TARI_TOKEN,
                "PieceWork agreements are funded in TARI only"
            );
            let amount = bucket.amount().to_u64_checked().expect("bucket amount out of range");
            assert!(amount > 0, "funding bucket must not be empty");
            assert!(
                self.funded_total + amount <= self.total_amount(),
                "funding exceeds the total milestone amount"
            );
            self.funded_total += amount;
            self.vault.deposit(bucket);
        }

        /// Mark a milestone as delivered. Only the payee may do this.
        pub fn mark_delivered(&mut self, index: u32) {
            self.assert_open();
            self.assert_payee();
            let milestone = self.milestone_mut(index);
            assert!(!milestone.released, "milestone already released");
            assert!(!milestone.delivered, "milestone already marked delivered");
            milestone.delivered = true;
        }

        /// Approve a delivered milestone, releasing exactly its amount from
        /// the escrow vault. Only the payer may approve, the milestone must
        /// have been marked delivered, it must not have been released before,
        /// and the vault must actually hold the funds (i.e. it was funded).
        /// Returns the released bucket for the caller to deposit to the payee.
        pub fn approve(&mut self, index: u32) -> Bucket {
            self.assert_open();
            self.assert_payer();
            let amount = {
                let milestone = self.milestone_mut(index);
                assert!(milestone.delivered, "milestone has not been marked delivered");
                assert!(!milestone.released, "milestone already released");
                milestone.amount
            };
            assert!(
                self.vault.balance() >= Amount::from_u64(amount),
                "agreement is not funded enough to release this milestone"
            );
            self.milestone_mut(index).released = true;
            self.released_total += amount;
            self.vault.withdraw(Amount::from_u64(amount))
        }

        /// Refund the whole unreleased remainder to the payer. Only the
        /// payer, only at or after the deadline epoch, and only while there
        /// is still something left in the vault. Closes the agreement.
        pub fn refund(&mut self) -> Bucket {
            self.assert_open();
            self.assert_payer();
            assert!(
                Consensus::current_epoch() >= self.deadline_epoch,
                "unreleased funds are only refundable at or after the deadline"
            );
            assert!(self.vault.balance() > Amount::zero(), "nothing left to refund");
            self.closed = true;
            self.vault.withdraw_all()
        }

        /// Cancel the agreement. Either party may cancel, but only while the
        /// agreement is completely unfunded.
        pub fn cancel(&mut self) {
            self.assert_open();
            let signer = CallerContext::transaction_signer_public_key();
            assert!(
                signer == self.payer || signer == self.payee,
                "only the payer or the payee can cancel the agreement"
            );
            assert!(
                self.funded_total == 0 && self.vault.balance() == Amount::zero(),
                "only an unfunded agreement can be cancelled"
            );
            self.cancelled = true;
        }

        // ----- read-only getters -----

        pub fn payer(&self) -> RistrettoPublicKeyBytes {
            self.payer
        }

        pub fn payee(&self) -> RistrettoPublicKeyBytes {
            self.payee
        }

        pub fn deadline_epoch(&self) -> u64 {
            self.deadline_epoch
        }

        pub fn total_amount(&self) -> u64 {
            self.milestones.iter().map(|m| m.amount).sum()
        }

        pub fn funded_amount(&self) -> u64 {
            self.funded_total
        }

        pub fn released_amount(&self) -> u64 {
            self.released_total
        }

        pub fn vault_balance(&self) -> u64 {
            self.vault.balance().to_u64_checked().unwrap_or(0)
        }

        pub fn milestone_count(&self) -> u32 {
            self.milestones.len() as u32
        }

        pub fn milestone_amount(&self, index: u32) -> u64 {
            self.milestone(index).amount
        }

        pub fn is_delivered(&self, index: u32) -> bool {
            self.milestone(index).delivered
        }

        pub fn is_released(&self, index: u32) -> bool {
            self.milestone(index).released
        }

        pub fn is_cancelled(&self) -> bool {
            self.cancelled
        }

        pub fn is_closed(&self) -> bool {
            self.closed
        }

        // ----- internal helpers -----

        fn milestone(&self, index: u32) -> &Milestone {
            self.milestones.get(index as usize).expect("milestone index out of range")
        }

        fn milestone_mut(&mut self, index: u32) -> &mut Milestone {
            self.milestones.get_mut(index as usize).expect("milestone index out of range")
        }

        fn assert_open(&self) {
            assert!(!self.cancelled, "agreement has been cancelled");
            assert!(!self.closed, "agreement has been closed by a refund");
        }

        fn assert_payer(&self) {
            assert!(
                CallerContext::transaction_signer_public_key() == self.payer,
                "only the payer can perform this action"
            );
        }

        fn assert_payee(&self) {
            assert!(
                CallerContext::transaction_signer_public_key() == self.payee,
                "only the payee can perform this action"
            );
        }
    }

    /// A single milestone of an agreement. Amounts are in micro-TARI (µT),
    /// the smallest unit of the native TARI token.
    pub struct Milestone {
        pub amount: u64,
        pub delivered: bool,
        pub released: bool,
    }
}
