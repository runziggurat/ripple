//! Contains test with ledger queries.
//! Queries and expected replies:
//!
//!     - mtGET_LEDGER -> mtLEDGER_DATA

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::{TmGetLedger, TmLedgerInfoType, TmLedgerType},
    },
    tests::conformance::{perform_expected_message_test, TestConfig},
};

#[tokio::test]
#[allow(non_snake_case)]
async fn c004_t1_TM_GET_LEDGER_LiBase_get_basic_info() {
    // ZG-CONFORMANCE-004
    let payload = Payload::TmGetLedger(TmGetLedger {
        itype: TmLedgerInfoType::LiBase as i32,
        ltype: Some(TmLedgerType::LtClosed as i32),
        ledger_hash: None,
        ledger_seq: None,
        node_i_ds: vec![],
        request_cookie: None,
        query_type: None,
        query_depth: None,
    });
    check_for_ledger_data_response(payload).await;
}

#[tokio::test]
#[allow(non_snake_case)]
async fn c004_t2_TM_GET_LEDGER_LiAsNode_get_account_state_info() {
    // ZG-CONFORMANCE-004
    let payload = Payload::TmGetLedger(TmGetLedger {
        itype: TmLedgerInfoType::LiAsNode as i32,
        ltype: Some(TmLedgerType::LtClosed as i32),
        ledger_hash: None,
        ledger_seq: None,
        // Anything other than itype = TmLedgerInfoType::LiBase above requires list of nodes' ids.
        // Here, only one node with id build from 0s to ease deserialize inside ripple.
        node_i_ds: vec![vec![0u8; 33]],
        request_cookie: None,
        query_type: None,
        query_depth: None,
    });
    check_for_ledger_data_response(payload).await;
}

async fn check_for_ledger_data_response(payload: Payload) {
    let check = |m: &BinaryMessage| matches!(&m.payload, Payload::TmLedgerData(..));
    perform_expected_message_test(TestConfig::default().with_initial_message(payload), &check)
        .await;
}
