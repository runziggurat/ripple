# Ziggurat x XRPL

The Ziggurat implementation for XRPLF's `rippled` nodes.

## Getting started

1. Clone this repository.
2. Build [rippled](https://github.com/XRPLF/rippled) from source.
3. Create the `~/.ziggurat/ripple/setup` directories. Next, create a file there named `validators.txt` with the following content:

```
#
# Default validators.txt
#
# This file is located in the same folder as your rippled.cfg file
# and defines which validators your server trusts not to collude.
#
# This file is UTF-8 with DOS, UNIX, or Mac style line endings.
# Blank lines and lines starting with a '#' are ignored.
#
#
#
# [validators]
#
#   List of the validation public keys of nodes to always accept as validators.
#
#   Manually listing validator keys is not recommended for production networks.
#   See validator_list_sites and validator_list_keys below.
#
#   Examples:
#    n9KorY8QtTdRx7TVDpwnG9NvyxsDwHUKUEeDLY3AkiGncVaSXZi5
#    n9MqiExBcoG19UXwoLjBJnhsxEhAZMuWwJDRdkyDz1EkEkwzQTNt
#
# [validator_list_sites]
#
#   List of URIs serving lists of recommended validators.
#
#   Examples:
#    https://vl.ripple.com
#    https://vl.coil.com
#    https://vl.xrplf.org
#    http://127.0.0.1:8000
#    file:///etc/opt/ripple/vl.txt
#
# [validator_list_keys]
#
#   List of keys belonging to trusted validator list publishers.
#   Validator lists fetched from configured sites will only be considered
#   if the list is accompanied by a valid signature from a trusted
#   publisher key.
#   Validator list keys should be hex-encoded.
#
#   Examples:
#    ED2677ABFFD1B33AC6FBC3062B71F1E8397C1505E1C42C64D11AD1B28FF73F4734
#    ED307A760EE34F2D0CAA103377B1969117C38B8AA0AA1E2A24DAC1F32FC97087ED
#

# The default validator list publishers that the rippled instance
# trusts.
#
# WARNING: Changing these values can cause your rippled instance to see a
#          validated ledger that contradicts other rippled instances'
#          validated ledgers (aka a ledger fork) if your validator list(s)
#          do not sufficiently overlap with the list(s) used by others.
#          See: https://arxiv.org/pdf/1802.07242.pdf

[validator_list_sites]
https://vl.ripple.com
https://vl.xrplf.org

[validator_list_keys]
#vl.ripple.com
ED2677ABFFD1B33AC6FBC3062B71F1E8397C1505E1C42C64D11AD1B28FF73F4734
# vl.xrplf.org
ED45D1840EE724BE327ABE9146503D5848EFD5F38B6D5FEDE71E80ACCE5E6E738B
# our localhost used in test c026()
02ED521B8124454DD5B7769C813BD40E8D36E134DD51ACED873B49E165327F6DF2

# To use the test network (see https://xrpl.org/connect-your-rippled-to-the-xrp-test-net.html),
# use the following configuration instead:
#
# [validator_list_sites]
# https://vl.altnet.rippletest.net
#
# [validator_list_keys]
# ED264807102805220DA0F312E71FC2C69E1552C9C5790F6C25E3729DEB573D5860

```
4. In the same directory create a `config.toml` with the following contents:
    ```
    path = "<path to the directory where you built rippled>"
    start_command = "./rippled"
    ```
5. Run tests with `cargo +stable t -- --test-threads=1`.

### Initial state
Specific tests require an initial node state to be set up.
Follow the steps below to save an initial state that can be loaded later in certain tests.

#### Preparation (needs to be done once)
1. Make sure you have python3 installed. You should be able to run `python3 --version`.
2. Install `xrpl` python lib: `pip3 install xrpl-py`.

##### Mac users
Make sure these two `127.0.0.x` (where `x != 1`) addresses are enabled:
```
    sudo ifconfig lo0 alias 127.0.0.2 up;
    sudo ifconfig lo0 alias 127.0.0.3 up;
```

#### Transferring XRP from the Genesis account to a new account and saving the state
1. In one terminal run test `cargo +stable t setup::testnet::test::run_testnet -- --ignored`.
   The test will start a local testnet and will keep it alive for 10 minutes. Ensure that you complete the
   following steps while above test is running.

2. Run `python3 tools/account_info.py` to monitor state of the accounts. 
   Wait until `ResponseStatus.SUCCESS` is reported for the genesis account. The response should include:
   ```
    "Account": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
    "Balance": "100000000000000000",
   ```
   This should happen within about a minute.
   Ignore error for the account `rNGknFCRBZguXcPqC63k6xTZnonSe6ZuWt` for the time being.
3. Run `python3 tools/transfer.py` to transfer xrp from genesis account to a new account.
4. Run `python3 tools/account_info.py` again to monitor accounts. The response for genesis account should include:
   ```
        "Account": "rHb9CJAWyB4rj91VRWn96DkukG4bwdtyTh",
        "Balance": "99999994999999990",
   ```
   and the response for the new account should include:
   ```
        "Account": "rNGknFCRBZguXcPqC63k6xTZnonSe6ZuWt",
        "Balance": "5000000000",
   ```
5. Copy the node's files to directory referenced by constant `pub const STATEFUL_NODES_DIR`, currently:
   ```
   cp -a ~/.ziggurat/ripple/testnet/ ~/.ziggurat/ripple/stateful;
   ```
6. Now you can stop the test started in step 1.
7. Perform cleanup:
   ```
   rm ~/.ziggurat/ripple/stateful/*/rippled.cfg;  # config files will be created when nodes are started
   rm -rf ~/.ziggurat/ripple/testnet;             # not needed anymore
   ```
