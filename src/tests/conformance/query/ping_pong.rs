//! Contains test with ping queries.
//! Queries and expected replies:
//!
//!     - mtPING (with PingType::PtPing) -> mtPING (with PingType::PtPong)

use rand::{thread_rng, RngCore};

use crate::{
    protocol::{
        codecs::message::{BinaryMessage, Payload},
        proto::{tm_ping::PingType, TmPing},
    },
    tests::conformance::{perform_expected_message_test, TestConfig},
};

#[tokio::test]
#[allow(non_snake_case)]
async fn c003_TM_PING_expect_pong() {
    // ZG-CONFORMANCE-003
    // Send `ping` message
    let seq = thread_rng().next_u32();

    let payload = Payload::TmPing(TmPing {
        r#type: PingType::PtPing as i32,
        seq: Some(seq),
        ping_time: None,
        net_time: None,
    });
    let check = |m: &BinaryMessage| {
        matches!(
            &m.payload,
            // proto file defines 'pong' message as `TmPing` with `r#type` set to [PingType::PtPong]
            Payload::TmPing(TmPing {
                r#type: r_type,
                seq: Some(s),
                ..
            }) if *s == seq && *r_type == PingType::PtPong as i32
        )
    };
    // Wait for reply
    perform_expected_message_test(TestConfig::default().with_initial_message(payload), &check)
        .await;
}
