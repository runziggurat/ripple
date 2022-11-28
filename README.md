# Ziggurat x XRPL

The Ziggurat implementation for XRPLF's `rippled` nodes.

## Getting started

1. Clone this repository.
2. Build [rippled](https://github.com/XRPLF/rippled) from source.
3. Create the `~/.ziggurat/ripple/setup` directories, and copy the `setup/validators.txt` file there.

```
cp setup/validators.txt ~/.ziggurat/ripple/setup
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
