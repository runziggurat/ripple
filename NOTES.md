# Notes
## This files serves as a scratch pad for future work and a single place with findings

### Messages and their operations (based on PeerImp.cpp from ripple source code at e5275b857752c2d6078cb8774edbb7e60f179d72)

#### Protocol start

    Based on `void PeerImp::doProtocolStart()` in PeerImp.cpp:
    <>
    <- loaded validators list
    <- optional: manifest message
    <- mtGET_PEER_SHARD_INFO_V2

#### Deprecated messages

    Ignores deprecated messages:
    <>
    -> TMGetPeerShardInfo
    -> TMPeerShardInfo
    -> TMGetPeerShardInfoV2

#### TMManifests
    (need to understand action better)

#### TMCluster
    Updates cluster

#### TMEndpoints
    asserts:
        connects to given endpoint if hops > 0

#### TMTransactions
        (not sure what to do: if not in sync then nothing happens with the transaction)

#### TMGetLedger
    asserts:
        responds with ledger data
        bad data for:
            invalid requested ledger info type
            no ledger hash for liTS_CANDIDATE
            invalid request (PeerImp.cpp: 1651)
            invalid requested ledger type
            invalid hash
            invalid ledger sequence
            invalid ledger node id (PeerImp.cpp: 1685)
            invalid query type (PeerImp.cpp: 1697)
            invalid query depth (currently 3)

#### TMLedgerData
    asserts:
        bad data for:
            invalid ledger hash
            invalid ledger sequence
            invalid ledger info type
            invalid reply error
            invalid ledger nodes 

#### TMProposeLedger
    (leads to ProposeSet, need to understand action better)

#### TMStatusChange
    (need to understand action better)

#### TMHaveSet
    (leads to TMHaveTransactionSet, need to understand action better)

#### TMValidation
    (need to understand action better)

#### TMGetObjects
    asserts:
        for type otTRANSACTIONS: responds with mtTRANSACTIONS
        for other: responds with mtGET_OBJECTS

#### mtSHARD_INFO: not found

#### TMValidatorList
    (need to understand action better, broadcasts list perhaps?)

#### TMSquelch
    (need to understand action better)

#### TMValidatorListCollection
    (need to understand action better, broadcasts list perhaps?)

#### TMProofPathRequest
    asserts:
        responds with mtPROOF_PATH_RESPONSE

#### TMProofPathResponse
    (need to understand action better, charge on some conditions)

#### TMReplayDeltaRequest
    asserts:
        responds with mtREPLAY_DELTA_RESPONSE

#### TMReplayDeltaResponse
    (need to understand action better, charge on some conditions)

#### TMGetPeerShardInfoV2
    asserts: 
        bad data if:
            relays > relayLimit (3)
            peerchain_size > relayLimit
            relays + peerchain_size > relayLimit
            invalid public key type
            public key unique in peer chain
        reply with mtPEER_SHARD_INFO_V2

#### TMPeerShardInfoV2
    asserts:
        forward message if a peer chain exists
        bad data if:
            timestamp > now + 5 seconds
            timestamp < now - 5 minutes
            incomplete.len() > latestShardIndex (PeerImp.cpp: 1311)
            shardIndex < earliestShardIndex || (PeerImp.cpp: 1321)
            invalid incomplete shard state (PeerImp.cpp: 1329)
            invalid incomplete shard progress ( < 1 || > 100) (PeerImp.cpp: 1329)
            duplicate incomplete shards (PeerImp.cpp: 1356)
            for finalized:
                empty 'finalized'
                invalid finalized indexes in string 'finalized'
                numFinalized == 0 || first finalized < earliestShardIndex || last finalized > latestShardIndex (PeerImp.cpp: 1378)
                numFinalized + numIncomplete > latestShardIndex (PeerImp.cpp: 1384)
            verify public key
            verify signature

#### TMHaveTransactions
        asserts:
            ignore message if any hash invalid
            responds with mtGET_OBJECTS if any transactions in request



