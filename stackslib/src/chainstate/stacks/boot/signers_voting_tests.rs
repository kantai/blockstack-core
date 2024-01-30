// Copyright (C) 2013-2020 Blockstack PBC, a public benefit corporation
// Copyright (C) 2020-2024 Stacks Open Internet Foundation
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::{TryFrom, TryInto};

use clarity::boot_util::boot_code_addr;
use clarity::vm::clarity::ClarityConnection;
use clarity::vm::contexts::OwnedEnvironment;
use clarity::vm::contracts::Contract;
use clarity::vm::costs::{CostOverflowingMath, LimitedCostTracker};
use clarity::vm::database::*;
use clarity::vm::errors::{
    CheckErrors, Error, IncomparableError, InterpreterError, InterpreterResult, RuntimeErrorType,
};
use clarity::vm::eval;
use clarity::vm::events::StacksTransactionEvent;
use clarity::vm::representations::SymbolicExpression;
use clarity::vm::tests::{execute, is_committed, is_err_code, symbols_from_values};
use clarity::vm::types::Value::Response;
use clarity::vm::types::{
    BuffData, OptionalData, PrincipalData, QualifiedContractIdentifier, ResponseData, SequenceData,
    StacksAddressExtensions, StandardPrincipalData, TupleData, TupleTypeSignature, TypeSignature,
    Value, NONE,
};
use stacks_common::address::AddressHashMode;
use stacks_common::types::chainstate::{
    BlockHeaderHash, BurnchainHeaderHash, StacksAddress, StacksBlockId, VRFSeed,
};
use stacks_common::types::Address;
use stacks_common::util::hash::{hex_bytes, to_hex, Sha256Sum, Sha512Trunc256Sum};
use stacks_common::util::secp256k1::Secp256k1PrivateKey;
use wsts::curve::point::{Compressed, Point};

use super::test::*;
use super::RawRewardSetEntry;
use crate::burnchains::{Burnchain, PoxConstants};
use crate::chainstate::burn::db::sortdb::{self, SortitionDB};
use crate::chainstate::burn::operations::*;
use crate::chainstate::burn::{BlockSnapshot, ConsensusHash};
use crate::chainstate::nakamoto::coordinator::tests::make_token_transfer;
use crate::chainstate::nakamoto::tests::get_account;
use crate::chainstate::nakamoto::tests::node::{TestSigners, TestStacker};
use crate::chainstate::nakamoto::NakamotoBlock;
use crate::chainstate::stacks::address::{PoxAddress, PoxAddressType20, PoxAddressType32};
use crate::chainstate::stacks::boot::pox_2_tests::{
    check_pox_print_event, generate_pox_clarity_value, get_reward_set_entries_at,
    get_stacking_state_pox, get_stx_account_at, with_clarity_db_ro, PoxPrintFields,
    StackingStateCheckData,
};
use crate::chainstate::stacks::boot::pox_4_tests::{
    assert_latest_was_burn, get_last_block_sender_transactions, get_tip, make_test_epochs_pox,
};
use crate::chainstate::stacks::boot::signers_tests::prepare_signers_test;
use crate::chainstate::stacks::boot::{
    BOOT_CODE_COST_VOTING_TESTNET as BOOT_CODE_COST_VOTING, BOOT_CODE_POX_TESTNET, SIGNERS_NAME,
    SIGNERS_VOTING_NAME,
};
use crate::chainstate::stacks::db::{
    MinerPaymentSchedule, StacksChainState, StacksHeaderInfo, MINER_REWARD_MATURITY,
};
use crate::chainstate::stacks::events::{StacksTransactionReceipt, TransactionOrigin};
use crate::chainstate::stacks::index::marf::MarfConnection;
use crate::chainstate::stacks::index::MarfTrieId;
use crate::chainstate::stacks::tests::make_coinbase;
use crate::chainstate::{self, stacks::*};
use crate::clarity_vm::clarity::{ClarityBlockConnection, Error as ClarityError};
use crate::clarity_vm::database::marf::{MarfedKV, WritableMarfStore};
use crate::clarity_vm::database::HeadersDBConn;
use crate::core::*;
use crate::net::test::{TestEventObserver, TestPeer};
use crate::util_lib::boot::boot_code_id;
use crate::util_lib::db::{DBConn, FromRow};

pub fn prepare_pox4_test<'a>(
    test_name: &str,
    observer: Option<&'a TestEventObserver>,
) -> (
    Burnchain,
    TestPeer<'a>,
    Vec<StacksPrivateKey>,
    StacksBlockId,
    u64,
    usize,
) {
    let (epochs, pox_constants) = make_test_epochs_pox();

    let mut burnchain = Burnchain::default_unittest(
        0,
        &BurnchainHeaderHash::from_hex(BITCOIN_REGTEST_FIRST_BLOCK_HASH).unwrap(),
    );
    burnchain.pox_constants = pox_constants.clone();

    let (mut peer, keys) =
        instantiate_pox_peer_with_epoch(&burnchain, test_name, Some(epochs.clone()), observer);

    assert_eq!(burnchain.pox_constants.reward_slots(), 6);
    let mut coinbase_nonce = 0;

    // Advance into pox4
    let target_height = burnchain.pox_constants.pox_4_activation_height;
    let mut latest_block = peer.tenure_with_txs(&[], &mut coinbase_nonce);
    while get_tip(peer.sortdb.as_ref()).block_height < u64::from(target_height) {
        latest_block = peer.tenure_with_txs(&[], &mut coinbase_nonce);
        // if we reach epoch 2.1, perform the check
        if get_tip(peer.sortdb.as_ref()).block_height > epochs[3].start_height {
            assert_latest_was_burn(&mut peer);
        }
    }

    let block_height = get_tip(peer.sortdb.as_ref()).block_height;

    info!("Block height: {}", block_height);

    (
        burnchain,
        peer,
        keys,
        latest_block,
        block_height,
        coinbase_nonce,
    )
}

#[test]
fn vote_for_aggregate_public_key() {
    let stacker_1 = TestStacker::from_seed(&[3, 4]);
    let stacker_2 = TestStacker::from_seed(&[5, 6]);
    let observer = TestEventObserver::new();

    let signer = key_to_stacks_addr(&stacker_1.signer_private_key).to_account_principal();

    let (mut peer, mut test_signers, latest_block_id) = prepare_signers_test(
        function_name!(),
        vec![(signer, 1000)],
        Some(vec![&stacker_1, &stacker_2]),
        Some(&observer),
    );

    let current_reward_cycle = readonly_call(
        &mut peer,
        &latest_block_id,
        SIGNERS_VOTING_NAME.into(),
        "current-reward-cycle".into(),
        vec![],
    )
    .expect_u128();

    assert_eq!(current_reward_cycle, 7);

    let last_set_cycle = readonly_call(
        &mut peer,
        &latest_block_id,
        SIGNERS_NAME.into(),
        "stackerdb-get-last-set-cycle".into(),
        vec![],
    )
    .expect_result_ok()
    .expect_u128();

    assert_eq!(last_set_cycle, 7);

    let signer_nonce = 0;
    let signer_key = &stacker_1.signer_private_key;
    let signer_address = key_to_stacks_addr(signer_key);
    let signer_principal = PrincipalData::from(signer_address);
    let cycle_id = current_reward_cycle;

    let signers = readonly_call(
        &mut peer,
        &latest_block_id,
        "signers".into(),
        "stackerdb-get-signer-slots".into(),
        vec![],
    )
    .expect_result_ok()
    .expect_list();

    let signer_index = signers
        .iter()
        .position(|value| {
            value
                .clone()
                .expect_tuple()
                .get("signer")
                .unwrap()
                .clone()
                .expect_principal()
                == signer_address.to_account_principal()
        })
        .expect("signer not found") as u128;

    let aggregated_public_key: Point = Point::new();

    let mut stacker_1_nonce: u64 = 1;
    let dummy_tx_1 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );
    let dummy_tx_2 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );
    let dummy_tx_3 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );
    let dummy_tx_4 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );
    let dummy_tx_5 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );
    let dummy_tx_6 = make_dummy_tx(
        &mut peer,
        &stacker_1.stacker_private_key,
        &mut stacker_1_nonce,
    );

    let txs = vec![
        // cast a vote for the aggregate public key
        make_signers_vote_for_aggregate_public_key(
            signer_key,
            signer_nonce,
            signer_index,
            &aggregated_public_key,
            0,
        ),
        // cast the vote twice
        make_signers_vote_for_aggregate_public_key(
            signer_key,
            signer_nonce + 1,
            signer_index,
            &aggregated_public_key,
            0,
        ),
    ];

    let txids: Vec<Txid> = txs.clone().iter().map(|t| t.txid()).collect();
    dbg!(txids);

    //
    // vote in the last burn block of prepare phase
    //

    nakamoto_tenure(
        &mut peer,
        &mut test_signers,
        vec![vec![dummy_tx_1]],
        signer_key,
    );

    nakamoto_tenure(
        &mut peer,
        &mut test_signers,
        vec![vec![dummy_tx_2]],
        signer_key,
    );

    // vote now
    let blocks_and_sizes = nakamoto_tenure(&mut peer, &mut test_signers, vec![txs], signer_key);
    let block = observer.get_blocks().last().unwrap().clone();
    let receipts = block.receipts.as_slice();
    assert_eq!(receipts.len(), 2);
    // ignore tenure change tx
    // ignore coinbase tx
    let tx1 = &receipts[receipts.len() - 2];
    assert_eq!(
        tx1.result,
        Value::Response(ResponseData {
            committed: true,
            data: Box::new(Value::Bool(true))
        })
    );

    let tx2 = &receipts[receipts.len() - 1];
    assert_eq!(
        tx2.result,
        Value::Response(ResponseData {
            committed: false,
            data: Box::new(Value::UInt(10006)) // err-duplicate-vote
        })
    );
}

fn nakamoto_tenure(
    peer: &mut TestPeer,
    test_signers: &mut TestSigners,
    txs_of_blocks: Vec<Vec<StacksTransaction>>,
    stacker_private_key: &StacksPrivateKey,
) -> Vec<(NakamotoBlock, u64, ExecutionCost)> {
    let current_height = peer.get_burnchain_view().unwrap().burn_block_height;

    info!("current height: {}", current_height);

    let (burn_ops, mut tenure_change, miner_key) =
        peer.begin_nakamoto_tenure(TenureChangeCause::BlockFound);

    let (_, _, consensus_hash) = peer.next_burnchain_block(burn_ops);

    let vrf_proof = peer.make_nakamoto_vrf_proof(miner_key);

    tenure_change.tenure_consensus_hash = consensus_hash.clone();
    tenure_change.burn_view_consensus_hash = consensus_hash.clone();
    let tenure_change_tx = peer
        .miner
        .make_nakamoto_tenure_change(tenure_change.clone());
    let coinbase_tx = peer.miner.make_nakamoto_coinbase(None, vrf_proof);
    let recipient_addr = boot_code_addr(false);
    let mut mutable_txs_of_blocks = txs_of_blocks.clone();
    mutable_txs_of_blocks.reverse();
    let blocks_and_sizes = peer.make_nakamoto_tenure(
        tenure_change_tx,
        coinbase_tx.clone(),
        test_signers,
        |miner, chainstate, sortdb, blocks| mutable_txs_of_blocks.pop().unwrap_or(vec![]),
    );
    info!("tenure length {}", blocks_and_sizes.len());
    blocks_and_sizes
}

fn make_dummy_tx(
    peer: &mut TestPeer,
    private_key: &StacksPrivateKey,
    nonce: &mut u64,
) -> StacksTransaction {
    peer.with_db_state(|sortdb, chainstate, _, _| {
        let addr = key_to_stacks_addr(&private_key);
        let account = get_account(chainstate, sortdb, &addr);
        let recipient_addr = boot_code_addr(false);
        let stx_transfer = make_token_transfer(
            chainstate,
            sortdb,
            &private_key,
            *nonce,
            1,
            1,
            &recipient_addr,
        );
        *nonce += 1;
        Ok(stx_transfer)
    })
    .unwrap()
}
