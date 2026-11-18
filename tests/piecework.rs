// Engine tests for the PieceWork template, run against the real Ootle engine
// in-process via tari_template_test_tooling. No network is involved.
//
// Covered rules (mirrored by the browser simulation in /docs):
//  * fund + approve releases exactly one milestone slice to the payee
//  * a non-payer cannot approve (stranger, and the payee cannot self-approve)
//  * a non-payee cannot mark a milestone delivered
//  * approving before delivery fails; double approval fails
//  * an unfunded agreement cannot release anything
//  * unreleased funds are refundable only at/after the deadline epoch,
//    and the refund returns exactly the unreleased remainder
//  * either party can cancel an unfunded agreement; a funded one cannot be
//    cancelled, and a cancelled agreement accepts no further actions

use ootle_byte_type::ToByteType;
use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
use tari_engine_types::virtual_substate::{VirtualSubstate, VirtualSubstateId};
use tari_ootle_transaction::args;
use tari_template_lib::types::{
    Amount, ComponentAddress, NonFungibleAddress, constants::TARI_TOKEN, crypto::RistrettoPublicKeyBytes,
};
use tari_template_test_tooling::TemplateTest;

// Milestone amounts in micro-TARI (µT). Funded test accounts start with
// 1_000_000_000 µT from the faucet, so these are comfortably affordable.
const M1: u64 = 300_000;
const M2: u64 = 200_000;
const TOTAL: u64 = M1 + M2;
const DEADLINE: u64 = 10;

struct Party {
    account: ComponentAddress,
    proof: NonFungibleAddress,
    secret: RistrettoSecretKey,
    public_key: RistrettoPublicKey,
}

impl Party {
    fn create(test: &mut TemplateTest) -> Self {
        let (account, proof, secret, public_key) = test.create_funded_account_with_keypair();
        Self { account, proof, secret, public_key }
    }

    fn pk_bytes(&self) -> RistrettoPublicKeyBytes {
        self.public_key.to_byte_type()
    }
}

struct Setup {
    test: TemplateTest,
    piecework: ComponentAddress,
    payer: Party,
    payee: Party,
    stranger: Party,
}

fn make_setup(milestones: Vec<u64>, deadline_epoch: u64) -> Setup {
    let mut test = TemplateTest::my_crate();
    let payer = Party::create(&mut test);
    let payee = Party::create(&mut test);
    let stranger = Party::create(&mut test);

    let template = test.get_template_address("PieceWork");
    test.execute_expect_success(
        test.transaction()
            .call_function(template, "new", args![payee.pk_bytes(), milestones, deadline_epoch])
            .build_and_seal(&payer.secret),
        vec![],
    );
    let piecework: ComponentAddress = test.expect_return(0);

    Setup { test, piecework, payer, payee, stranger }
}

fn balance(test: &mut TemplateTest, party: &Party) -> u64 {
    let amount: Amount = test.call_method(party.account, "balance", args![TARI_TOKEN], vec![party.proof.clone()]);
    amount.to_u64_checked().expect("account balance out of range")
}

fn vault_balance(test: &mut TemplateTest, piecework: ComponentAddress) -> u64 {
    test.call_method(piecework, "vault_balance", args![], vec![])
}

fn fund(setup: &mut Setup, amount: u64) {
    let piecework = setup.piecework;
    let payer_account = setup.payer.account;
    let payer_secret = setup.payer.secret.clone();
    let payer_proof = setup.payer.proof.clone();
    setup.test.execute_expect_success(
        setup
            .test
            .transaction()
            .call_method(payer_account, "withdraw", args![TARI_TOKEN, amount])
            .put_last_instruction_output_on_workspace("funding")
            .call_method(piecework, "fund", args![Workspace("funding")])
            .build_and_seal(&payer_secret),
        vec![payer_proof],
    );
}

fn mark_delivered(setup: &mut Setup, signer: &RistrettoSecretKey, index: u32) {
    let piecework = setup.piecework;
    setup.test.execute_expect_success(
        setup.test.transaction().call_method(piecework, "mark_delivered", args![index]).build_and_seal(signer),
        vec![],
    );
}

fn mark_delivered_expect_failure(setup: &mut Setup, signer: &RistrettoSecretKey, index: u32) {
    let piecework = setup.piecework;
    setup.test.execute_expect_failure(
        setup.test.transaction().call_method(piecework, "mark_delivered", args![index]).build_and_seal(signer),
        vec![],
    );
}

/// Approve milestone 0, routing the released bucket into the payee's
/// account. Signed by whoever `signer` is, so tests can use the wrong party.
fn approve(setup: &mut Setup, signer: &RistrettoSecretKey) {
    let piecework = setup.piecework;
    let payee_account = setup.payee.account;
    setup.test.execute_expect_success(
        setup
            .test
            .transaction()
            .call_method(piecework, "approve", args![0u32])
            .put_last_instruction_output_on_workspace("payout")
            .call_method(payee_account, "deposit", args![Workspace("payout")])
            .build_and_seal(signer),
        vec![],
    );
}

fn approve_expect_failure(setup: &mut Setup, signer: &RistrettoSecretKey) {
    let piecework = setup.piecework;
    let payee_account = setup.payee.account;
    setup.test.execute_expect_failure(
        setup
            .test
            .transaction()
            .call_method(piecework, "approve", args![0u32])
            .put_last_instruction_output_on_workspace("payout")
            .call_method(payee_account, "deposit", args![Workspace("payout")])
            .build_and_seal(signer),
        vec![],
    );
}

fn refund(setup: &mut Setup, signer: &RistrettoSecretKey) {
    let piecework = setup.piecework;
    let payer_account = setup.payer.account;
    setup.test.execute_expect_success(
        setup
            .test
            .transaction()
            .call_method(piecework, "refund", args![])
            .put_last_instruction_output_on_workspace("refund")
            .call_method(payer_account, "deposit", args![Workspace("refund")])
            .build_and_seal(signer),
        vec![],
    );
}

fn refund_expect_failure(setup: &mut Setup, signer: &RistrettoSecretKey) {
    let piecework = setup.piecework;
    let payer_account = setup.payer.account;
    setup.test.execute_expect_failure(
        setup
            .test
            .transaction()
            .call_method(piecework, "refund", args![])
            .put_last_instruction_output_on_workspace("refund")
            .call_method(payer_account, "deposit", args![Workspace("refund")])
            .build_and_seal(signer),
        vec![],
    );
}

fn set_epoch(test: &mut TemplateTest, epoch: u64) {
    test.set_virtual_substate(VirtualSubstateId::CurrentEpoch, VirtualSubstate::CurrentEpoch(epoch));
}

#[test]
fn fund_and_approve_releases_exactly_one_milestone_slice() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    let payee_secret = setup.payee.secret.clone();
    let payee_before = balance(&mut setup.test, &setup.payee);
    let payer_before = balance(&mut setup.test, &setup.payer);

    fund(&mut setup, TOTAL);
    assert_eq!(vault_balance(&mut setup.test, setup.piecework), TOTAL);

    mark_delivered(&mut setup, &payee_secret, 0);
    approve(&mut setup, &payer_secret);

    // Exactly milestone 0's slice moved to the payee; milestone 1 is untouched.
    assert_eq!(balance(&mut setup.test, &setup.payee), payee_before + M1);
    assert_eq!(balance(&mut setup.test, &setup.payer), payer_before - TOTAL);
    assert_eq!(vault_balance(&mut setup.test, setup.piecework), M2);
    let released: u64 = setup.test.call_method(setup.piecework, "released_amount", args![], vec![]);
    assert_eq!(released, M1);
    let is_released: bool = setup.test.call_method(setup.piecework, "is_released", args![0u32], vec![]);
    assert!(is_released);
    let second_released: bool = setup.test.call_method(setup.piecework, "is_released", args![1u32], vec![]);
    assert!(!second_released);
}

#[test]
fn non_payer_cannot_approve() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payee_secret = setup.payee.secret.clone();
    let stranger_secret = setup.stranger.secret.clone();
    fund(&mut setup, TOTAL);
    mark_delivered(&mut setup, &payee_secret, 0);

    // A third party that is neither payer nor payee cannot approve.
    approve_expect_failure(&mut setup, &stranger_secret);
}

#[test]
fn payee_cannot_self_approve() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payee_secret = setup.payee.secret.clone();
    fund(&mut setup, TOTAL);
    mark_delivered(&mut setup, &payee_secret, 0);

    // The payee delivered the work but cannot release their own payment.
    approve_expect_failure(&mut setup, &payee_secret);
}

#[test]
fn non_payee_cannot_mark_delivered() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    let stranger_secret = setup.stranger.secret.clone();
    fund(&mut setup, TOTAL);

    mark_delivered_expect_failure(&mut setup, &payer_secret, 0);
    mark_delivered_expect_failure(&mut setup, &stranger_secret, 0);
}

#[test]
fn approve_before_delivery_fails() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    fund(&mut setup, TOTAL);

    // Funded, but the payee never marked milestone 0 delivered.
    approve_expect_failure(&mut setup, &payer_secret);
}

#[test]
fn double_approval_fails() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    let payee_secret = setup.payee.secret.clone();
    fund(&mut setup, TOTAL);
    mark_delivered(&mut setup, &payee_secret, 0);
    approve(&mut setup, &payer_secret);

    // Second approval of the same milestone must fail.
    approve_expect_failure(&mut setup, &payer_secret);
    assert_eq!(vault_balance(&mut setup.test, setup.piecework), M2);
}

#[test]
fn unfunded_agreement_cannot_release() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    let payee_secret = setup.payee.secret.clone();
    // Delivered but never funded: approval must fail, nothing can be released.
    mark_delivered(&mut setup, &payee_secret, 0);
    approve_expect_failure(&mut setup, &payer_secret);
    assert_eq!(vault_balance(&mut setup.test, setup.piecework), 0);
}

#[test]
fn refund_only_after_deadline_and_returns_unreleased_remainder() {
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payer_secret = setup.payer.secret.clone();
    let payee_secret = setup.payee.secret.clone();
    let payer_before = balance(&mut setup.test, &setup.payer);
    fund(&mut setup, TOTAL);
    mark_delivered(&mut setup, &payee_secret, 0);
    approve(&mut setup, &payer_secret);

    // Before the deadline, refund must fail.
    refund_expect_failure(&mut setup, &payer_secret);

    // At the deadline, the unreleased remainder (milestone 1) comes back.
    set_epoch(&mut setup.test, DEADLINE);
    refund(&mut setup, &payer_secret);

    assert_eq!(balance(&mut setup.test, &setup.payer), payer_before - TOTAL + M2);
    assert_eq!(vault_balance(&mut setup.test, setup.piecework), 0);
    let closed: bool = setup.test.call_method(setup.piecework, "is_closed", args![], vec![]);
    assert!(closed);
}

#[test]
fn cancel_rules() {
    // Either party can cancel an unfunded agreement...
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let payee_secret = setup.payee.secret.clone();
    let piecework = setup.piecework;
    setup.test.execute_expect_success(
        setup.test.transaction().call_method(piecework, "cancel", args![]).build_and_seal(&payee_secret),
        vec![],
    );
    let cancelled: bool = setup.test.call_method(setup.piecework, "is_cancelled", args![], vec![]);
    assert!(cancelled);

    // ...a stranger cannot cancel...
    let mut setup = make_setup(vec![M1, M2], DEADLINE);
    let stranger_secret = setup.stranger.secret.clone();
    let piecework = setup.piecework;
    setup.test.execute_expect_failure(
        setup.test.transaction().call_method(piecework, "cancel", args![]).build_and_seal(&stranger_secret),
        vec![],
    );

    // ...and a funded agreement cannot be cancelled at all.
    let payer_secret = setup.payer.secret.clone();
    fund(&mut setup, TOTAL);
    let piecework = setup.piecework;
    setup.test.execute_expect_failure(
        setup.test.transaction().call_method(piecework, "cancel", args![]).build_and_seal(&payer_secret),
        vec![],
    );
}
