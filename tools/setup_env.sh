#!/usr/bin/env bash
# This script sets up the environment for the Ziggurat test suite.

# The subnet to add to loopback devices
SUBNET="1.1.1.0/24"

# Rippled files
if [ -z "$RIPPLED_BIN_PATH" ]; then
    echo "Aborting. Export RIPPLED_BIN_PATH before running this script."
    exit 1
fi
RIPPLED_BIN_NAME="rippled"

# Ziggurat config files
ZIGGURAT_RIPPLED_DIR="$HOME/.ziggurat/ripple"
ZIGGURAT_RIPPLED_SETUP_DIR="$ZIGGURAT_RIPPLED_DIR/setup"
ZIGGURAT_RIPPLED_SETUP_CFG_FILE="$ZIGGURAT_RIPPLED_SETUP_DIR/config.toml"
ZIGGURAT_RIPPLED_TESTNET_DIR="$ZIGGURAT_RIPPLED_DIR/testnet"
ZIGGURAT_RIPPLED_STATEFUL_DIR="$ZIGGURAT_RIPPLED_DIR/stateful"

setup_config_file() {
    echo "--- Setting up configuration file"
    echo "Creating $ZIGGURAT_RIPPLED_SETUP_CFG_FILE with contents:"
    mkdir -p $ZIGGURAT_RIPPLED_SETUP_DIR
    echo
    echo "# Rippled installation path" > $ZIGGURAT_RIPPLED_SETUP_CFG_FILE
    echo "path = \"$RIPPLED_BIN_PATH\"" >> $ZIGGURAT_RIPPLED_SETUP_CFG_FILE
    echo "# Start command with possible arguments" >> $ZIGGURAT_RIPPLED_SETUP_CFG_FILE
    echo "start_command = \"./$RIPPLED_BIN_NAME\"" >> $ZIGGURAT_RIPPLED_SETUP_CFG_FILE

    # Print file contents so the user can check whether the path is correct
    cat $ZIGGURAT_RIPPLED_SETUP_CFG_FILE
    echo
}

query_account_info() {
    if [ $(uname) == "Linux" ]; then
        TIMEOUT_CMD="timeout"
    elif [ $(uname) == "Darwin" ]; then
        TIMEOUT_CMD="gtimeout"
    fi

    # Run account query until it responds with "ResponseStatus.SUCCESS" or MAX_ATTEMPTS is reached
    TIMEOUT_SEC=5
    MAX_ATTEMPTS=5
    NUM_ATTEMPTS=0

    sleep $TIMEOUT_SEC
    until [ $NUM_ATTEMPTS -gt $(($MAX_ATTEMPTS-1)) ] \
        || $TIMEOUT_CMD $TIMEOUT_SEC python3 tools/account_info.py | grep "ResponseStatus.SUCCESS"; do
        ((NUM_ATTEMPTS++))
        echo "Query failed, number of attempts made: $NUM_ATTEMPTS"
        echo "Retrying..."
        sleep $TIMEOUT_SEC
    done
    if [ $NUM_ATTEMPTS -gt $(($MAX_ATTEMPTS-1)) ]; then
        echo "Could not establish a connection with the genesis account. Please try again."
        exit 1
    fi
    echo "Established a connection with the genesis account"
    return 0
}

setup_stateful_nodes() {
    # Query only after a long delay to account for compilation times and network preparation work
    ACCOUNT_QUERY_DELAY_SEC=300

    echo "--- Setting up initial node state, takes at least 5 minutes"
    echo
    echo "Spinning up a node instance, please be patient"
    cargo t setup::testnet::test::run_testnet -- --ignored &
    echo
    sleep $ACCOUNT_QUERY_DELAY_SEC
    echo "--- Querying account info"
    query_account_info
    echo
    echo "--- Executing transfer script"
    python3 tools/transfer.py
    # Copy the node's files to directory referenced by constant pub const STATEFUL_NODES_DIR
    cp -a $ZIGGURAT_RIPPLED_TESTNET_DIR $ZIGGURAT_RIPPLED_STATEFUL_DIR
    echo
    echo "--- Gracefully stopping the network"
    kill %1
    echo "--- Performing cleanup"
    # Remove unneeded and temporary files
    rm $ZIGGURAT_RIPPLED_STATEFUL_DIR/*/rippled.cfg
    rm -rf $ZIGGURAT_RIPPLED_TESTNET_DIR
    echo
}

# Verify the repo location
if [ "$(git rev-parse --is-inside-work-tree 2>/dev/null)" != "true" ]; then
    echo "Aborting. Use this script only from the ziggurat/xrpl repo."
    exit 1
fi
REPO_ROOT=`git rev-parse --show-toplevel`
if [ "`basename $REPO_ROOT`" != "xrpl" ]; then
    # Wrong root directory, check for rename compared to origin url.
    ORIGIN_URL=$(git config --local remote.origin.url|sed -n 's#.*/\([^.]*\)\.git#\1#p')
    if [ "$ORIGIN_URL" != "xrpl" ]; then
        echo "Aborting. Use this script only from the ziggurat/xrpl repo."
        exit 1
    fi
fi

# Remove already present ziggurat directory to ensure a fresh start
rm -rf $ZIGGURAT_RIPPLED_DIR

# Change dir to ensure script paths are always correct
pushd . &> /dev/null
cd $REPO_ROOT

setup_config_file
cp setup/validators.txt $ZIGGURAT_RIPPLED_SETUP_DIR
setup_stateful_nodes
echo "--- Setup successful"

popd &> /dev/null
