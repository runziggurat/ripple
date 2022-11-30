# Introduction

The purpose of this index is to provide an overview of the testing approaches to be implemented by Ziggurat. It is intended to evolve as the framework matures, leaving room for novel cases and extensions of existing cases, as called for by any protocol idiosyncrasies that may come to light during the development process.

Some test cases have been consolidated when similar behaviour is tested with differing messages. The final implementation of these cases will be subject to factors such as node setup and teardown details, test run time (and potentially runtime) constraints, readability and maintainability.

## Special Considerations

Some of these tests can be executed against the same node instance without resetting or clearing the cache to cut down on node setup and teardown times. Other tests are intended for use where a new peer is needed to clean the slate for a deterministic output.

For load testing, "reasonable load" and "heavy load" will need to be defined.

## Usage

The tests can be run with `cargo test` once Ziggurat is properly configured and dependencies (node instance to be tested) are satisfied. See the [README](README.md) for details.

Tests are grouped into the following categories: conformance, performance, and resistance. Each test is named after the category it belongs to, in addition to what's being tested. For example, `c001_handshake_when_node_receives_connection` is the first conformance test and tests the handshake behavior on the receiving end. The full naming convention is: `id_part_t(subtest_no)_(message type)_(extra_test_desc)`.

# Types of Tests

## Conformance

The conformance tests aim to verify the node adheres to the network protocol. In addition, they include some naive error cases with malicious and fuzzing cases consigned to the resistance tests. Most cases in this section will only require a socket standing in for the connected peer and a full node running in the background.

### Handshake

These tests verify the proper execution of a handshake between a node and a peer as well as some simple error cases.

### Post-handshake messages

These tests verify the node responds with the correct messages to requests and disconnects in certain trivial non-fuzz, non-malicious cases. These form the basic assumptions necessary for peering and syncing.

### Unsolicited post-handshake messages

These tests aim to evaluate the proper behaviour of a node when receiving unsolicited messages post-handshake.

### Simple peering

These tests evaluate the node's basic peering properties by verifying the data included in the messages are in accordance with the peering status of the node.

### Simple sync

These tests evaluate the node's basic syncing properties for transactions and blocks by verifying the data included in the message payloads are in accordance with the ranges provided by the peer.

## Performance

The performance tests aim to verify the node maintains a healthy throughput under pressure. This is principally done through simulating load with synthetic peers and evaluating the node's responsiveness. Synthetic peers will need to be able to simulate the behaviour of a full node by implementing handshaking, message sending and receiving.

### Load testing

These tests are intended to verify the node remains healthy under "reasonable load". Additionally these tests will be pushed to the extreme for resistance testing with heavier loads.

### Heavy load testing

These tests are meant to explore the impact of malicious network use against a node.

The amount of load and its frequency could be modulated to provide a comprehensive verification of the node's behaviour under different conditions (including synchronized requests from different peers and other worst case scenarios).

## Resistance

The resistance tests are designed for the early detection and avoidance of weaknesses exploitable through malicious behaviour. They attempt to probe boundary conditions with comprehensive fuzz testing and extreme load testing. The nature of the peers in these cases will depend on how accurately they needs to simulate node behaviour. It will likely be a mixture of simple sockets for the simple cases and peers used in the performance tests for the more advanced.

### Fuzz testing

The fuzz tests aim to buttress the message conformance tests with extra verification of expected node behaviour when receiving corrupted or broken messages. Our approach is targeting these specific areas and we anticipate broadening these test scenarios as necessary:

- Messages with any length and any content (random bytes).
- Messages with plausible lengths, e.g. 24 bytes for header and within the expected range for the body.
- Metadata-compliant messages, e.g. correct header, random body.
- Slightly corrupted but otherwise valid messages, e.g. N% of body replaced with random bytes.
- Messages with an incorrect checksum.
- Messages with differing announced and actual lengths.

# Test Index

The test index makes use of symbolic language in describing connection and message sending directions.

| Symbol | Meaning                                                             |
|--------|---------------------------------------------------------------------|
| `-> A` | Ziggurat's synthetic node sends a message `A` to Rippled            |
| `<- B` | Rippled sends a message `B` to Ziggurat's synthetic node            |
| `>> C` | Ziggurat's synthetic node broadcasts a message `C` to all its peers |
| `<< D` | Rippled broadcasts a message `D` to all its peers                   |
| `<>`   | Signifies a completed handshake, in either direction                |

## Conformance

### ZG-CONFORMANCE-001

    The node correctly performs a handshake from the responder side.

    ->
    -> public key & session signature
    <- public key & session signature

    Assert: the node’s peer count has increased to 1 and the synthetic node is an established peer.

### ZG-CONFORMANCE-002

    The node correctly performs a handshake from the initiator side.

    <-
    <- public key & session signature
    -> public key & session signature

    Assert: the node’s peer count has increased to 1 and the synthetic node is an established peer.

### ZG-CONFORMANCE-003

    The node responds with `pong` message for `ping`.

    <>
    -> ping message with random `sequence` number
    <- pong response with the same `sequence` number

### ZG-CONFORMANCE-004

    The node responds with mtLEDGER_DATA for mtGET_LEDGER with different iType types.
    iType types used here are LiBase and LiAsNode.

    <>
    -> mtGET_LEDGER (iType)
    <- mtLEDGER_DATA

### ZG-CONFORMANCE-005

    The node requests mtGET_PEER_SHARD_INFO_V2 after connection and handshake.

    <>
    <- mtGET_PEER_SHARD_INFO_V2

### ZG-CONFORMANCE-006

    The node should *NOT* send any messages after connection if there was no handshake.
    The test waits for the predefined amount of time, ensuring no messages were received.

### ZG-CONFORMANCE-007

    The node should respond with transaction details after receiveing mtGET_OBJECTS / OtTransactions request.
    Normally the node does not respond with transaction details if the transaction is not in its cache. In this test we first
    query for transaction details via rpc, then via peer protocol.

    <>
    -> mtGET_OBJECTS with r#type == OtTransactions
    <- mtTRANSACTIONS

### ZG-CONFORMANCE-008

    The node should query for the transaction object after receiving a mtHAVE_TRANSACTIONS packet.

    <>
    -> mtHAVE_TRANSACTIONS
    <- mtGET_OBJECTS

### ZG-CONFORMANCE-009

    The node should ignore the squelch message for its validator public key.

    <>
    <- mtPROPOSE_LEDGER (node public key 1)
    -> mtSQUELCH (node public key 1)
    <- mtPROPOSE_LEDGER (node public key 1)

    Assert: the synthetic node has continued receiving mtPROPOSE_LEDGER messages.

### ZG-CONFORMANCE-010

    The node should send mtSTATUS_CHANGE message containing ledger info.
    This message should be sent without any explicit requests.
    To ensure ledger correctness, the test asks for its information via RPC first 
    and compares the results with the mtSTATUS_CHANGE payload. 

    <>
    <- mtSTATUS_CHANGE with the correct ledger hash and sequence

### ZG-CONFORMANCE-011

    The node should relay a mtGET_PEER_SHARD_INFO_V2 message to connected peers
    if it contains a valid key type and relay counter is higher than 0.
    Connection scenario:
    Synthetic Node 1 <> Rippled <> Synthetic Node 2
    This test checks whether Synthetic node 2 receives the mtGET_PEER_SHARD_INFO_V2 
    message sent from Synthetic Node 1 to the Ripple node.

### ZG-CONFORMANCE-012

    The node should not relay a mtGET_PEER_SHARD_INFO_V2 message to connected peers
    if it contains unsupported key type.
    Connection scenario:
    Synthetic Node 1 <> Rippled <> Synthetic Node 2
    This test ensures that Synthetic node 2 does not receive the mtGET_PEER_SHARD_INFO_V2 
    message sent from Synthetic Node 1 to the Ripple node.

### ZG-CONFORMANCE-013

    The node should not relay a mtGET_PEER_SHARD_INFO_V2 message to connected peers
    if the message's relay count is equal 0.
    Connection scenario:
    Synthetic Node 1 <> Rippled <> Synthetic Node 2
    This test ensures that Synthetic node 2 does not receive the mtGET_PEER_SHARD_INFO_V2 
    message sent from Synthetic Node 1 to the Ripple node.

### ZG-CONFORMANCE-014

    The node should not relay a mtGET_PEER_SHARD_INFO_V2 message to connected peers
    if the message's relay count is above relay limit (currently 3).
    Connection scenario:
    Synthetic Node 1 <> Rippled <> Synthetic Node 2
    This test ensures that Synthetic node 2 does not receive the mtGET_PEER_SHARD_INFO_V2 
    message sent from Synthetic Node 1 to the Ripple node.

### ZG-CONFORMANCE-015

    The node should send a mtVALIDATORLISTCOLLECTION message containing at least one validator
    with a correct public key and a non-empty manifest.

    <>
    <- mtVALIDATORLISTCOLLECTION with at least one validator with a correct public key and a non-empty manifest.

### ZG-CONFORMANCE-016

    Deploy a multi-node network setup with nodes 1, 2 and 3.
    Connect a synthetic node to Node 1.

    Let A be a list of node public keys 1, 2 and 3.
    Let B be a list of node public keys 2 and 3.

    <> with Node1
    << mtPROPOSE_LEDGER (A)
    -> mtSQUELCH (B)
    << mtPROPOSE_LEDGER (A)

    Assert: A synthetic node receives only mtPROPOSE_LEDGER messages with a key from node 1
    after squelching node public keys belonging to nodes 2 and 3 (B).

### ZG-CONFORMANCE-017

    The node sends mtMANIFESTS after the handshake.

    <>
    <- mtMANIFESTS

### ZG-CONFORMANCE-018

    The node sends mtENDPOINTS after the handshake.

    <>
    <- mtENDPOINTS

### ZG-CONFORMANCE-019

    Nodes in the testnet should relay a mtTRANSACTION message to connected peers.
    Connection scenario:
    RPC call > Rippled 1 <> Rippled 2 <> Synthetic Node 
    This test checks whether the synthetic node receives the mtTRANSACTION 
    message containing details from the RPC call.    

### ZG-CONFORMANCE-020

    Nodes in the testnet should broadcast a mtHAVE_SET message to connected peers after a transaction.
    Connection scenario:
    Submit transaction via RPC call > Rippled 1 <> Rippled 2 <> Synthetic Node
    This test checks whether the synthetic node receives the mtHAVE_SET message.

### ZG-CONFORMANCE-021

    The node sends mtVALIDATION after the handshake.

    <>
    <- mtVALIDATION

### ZG-CONFORMANCE-022

    The node should respond with mtREPLAY_DELTA_RESPONSE to mtREPLAY_DELTA_REQ.
    During this test a feature 'ledgerReplay' is enabled. This requires two actions:
    1. Enabling the feature in the config file (option `[ledger_replay]` set to `1`).
    2. Adding `ledgerreplay=1` to the `X-Protocol-Ctl` header during the handshake.

    <>
    -> mtREPLAY_DELTA_REQ
    <- mtREPLAY_DELTA_RESPONSE

### ZG-CONFORMANCE-023

    The node should respond with mtPEER_SHARD_INFO_V2 to mtGET_PEER_SHARD_INFO_V2 when
    sharding is enabled.

    <>
    -> mtGET_PEER_SHARD_INFO_V2
    <- mtPEER_SHARD_INFO_V2

### ZG-CONFORMANCE-024

    The node should connect with other nodes in its cluster and exchange public keys.

    <-
    <- mtCLUSTER with public keys

### ZG-CONFORMANCE-025

    The node should respond with mtPROOF_PATH_RESPONSE to mtPROOF_PATH_REQ.

    <>
    -> mtPROOF_PATH_REQ
    <- mtPROOF_PATH_RESPONSE

### ZG-CONFORMANCE-026

    The test client should send a valid mtVALIDATORLIST message, with both master and signature public keys, correctly serializing a manifest and validator blob.
    The create is done, but the result is not yet checked.  That will be done in a future task/PR.

    <>
    <- mtVALIDATORLIST sent with master and signing public keys and a correctly serialized manifest and validator blob.

## Performance

### ZG-PERFORMANCE-001

    The node behaves as expected under load from other peers.
    1. Establish a node and synthetic peers.
    2. Begin simulation.
    3. Introspect node health and responsiveness through peers (latency, throughput). This could be done using `Ping`/`Pong` messages.
    There can be different errors during testing: broken pipes to signal established but suddenly lost connections,
    InvalidData errors to indicate that connection couldn't be established at all or timeout errors to indicate
    that data has not been received in timely manner.

## Resistance

### ZG-RESISTANCE-001

    The node rejects a handshake when the 'User-Agent' (for initiation side) or 'Server' (from responder side) header 
    is too long (8192 bytes in this test).
    The node should be able to accept connections after such a request.
    These tests attempt a handshake with long 'User-Agent'/'Server' headers and ensures that the connection
    is rejected. Then, it attempts a normal connection and ensures that the connection is established.

### ZG-RESISTANCE-002

    The node rejects various random bytes post-handshake.
    The test sends random bytes of variable length (between 1 and 65536) 20 times in a row.
    The random values are generated using rand_chacha for performance benefits and ease of use.

    <>
    -> random bytes
    
    Assert: The node is disconnected after sending random bytes

### ZG-RESISTANCE-003

    The node rejects the handshake when there is a bit flip in either a public_key or a shared_value (which later results in
    an invalid session signature). This should happen for both types of connection: Initiator and Responder.
    These tests attempt handshakes with intentionally broken public_key and signature and assert whether
    rippled dropped connection after some short time.


### ZG-RESISTANCE-004

    The node rejects various random bytes pre-handshake.

    -> random bytes
    
    Assert: The node is disconnected after sending random bytes
