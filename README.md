# Ziggurat x XRPL
[![dependency status](https://deps.rs/repo/github/runziggurat/xrpl/status.svg)](https://deps.rs/repo/github/runziggurat/xrpl)
![workflow status](https://github.com/runziggurat/xrpl/actions/workflows/rippled.yml/badge.svg)

The Ziggurat implementation for XRPLF's `rippled` nodes.

## Prerequisites
Ziggurat is written in stable Rust; you can install the Rust toolchain by following the official instructions [here](https://www.rust-lang.org/learn/get-started)

## Getting started

### Preconditions
1. Clone this repository.
2. Build [rippled](https://github.com/XRPLF/rippled) from source.

#### Running setup script
3. Make sure you have python3 installed. You should be able to run `python3 --version`.
4. Install `xrpl` python lib: `pip3 install xrpl-py`.

   ##### **Mandatory step for Mac users!**
   On MacOS, the `coreutils` package is also required. It can be installed with: `brew install coreutils`

   Also make sure that these two `127.0.0.x` (where `x != 1`) addresses are enabled:
   ```bash
   sudo ifconfig lo0 alias 127.0.0.2 up;
   sudo ifconfig lo0 alias 127.0.0.3 up;
   ```

   ##### **Important note!**
   The `xrlp` library depends on legacy openSSL functions that are disabled by default. In the case of error, make sure to explicitly enable legacy functions in the config file, like so:
   ```
   openssl_conf = openssl_init
   
   [openssl_init]
   providers = provider_sect
   
   [provider_sect]
   default = default_sect
   legacy = legacy_sect
   
   [default_sect]
   activate = 1
   
   [legacy_sect]
   activate = 1
   ```
   You can find out the path to the config file with the following command: `openssl version -d`.

5. Export the path to the build folder to the `RIPPLED_BIN_PATH` environment variable.
   ```bash
   export RIPPLED_BIN_PATH="$HOME/path/to/ripple"
6. Run the setup script (takes about 5 minutes):
   ```bash
   ./tools/setup_env.sh
   ```

#### Run tests
Run conformance and resistance tests with the following command:
```bash
cargo +stable t -- --test-threads=1
```
### Run performance tests
Create a package of IP addresses which are required for performance tests.

You must first fetch the `ips.py` script from the ziggurat-core repository.  Run this:

```bash
wget -O tools/ips.py https://raw.githubusercontent.com/runziggurat/ziggurat-core/main/ziggurat-core-scripts/ips.py
```

_NOTE: To run the `ips.py` script below, the user must be in the sudoers file in order to use this script.
Script uses `ip`/`ipconfig` commands which require sudo privilages._

From the root repository directory, depending on your OS, run one of the following commands.

#### Preconditions under Linux
Generate dummy devices with addresses:
```bash
python3 ./tools/ips.py --subnet 1.1.1.0/24 --file src/tools/ips.rs --dev_prefix test_zeth
```

#### Preconditions under MacOS
Add the whole subnet to the loopback device - can also be used on Linux (device name - Linux: `lo`, MacOS: `lo0`):
```bash
python3 ./tools/ips.py --subnet 1.1.0.0/24 --file src/tools/ips.rs --dev lo0
```

Read ./tools/ips.py for more details.

#### Run tests
Run performance tests with the following command:
```bash
cargo +stable t performance --features performance -- --test-threads=1
```

## Test status

Short overview of test cases and their current status. In case of failure, the behaviour observed is usually documented in the test case.
These results were obtained by running the test suite against [Ripple 1.9.3](https://github.com/XRPLF/rippled) (47dec467).

| Status |               |
|:------:|---------------|
|   ✓    | pass          |
|   ✖    | fail          |


### Conformance

|             Test Case             | Status | Additional Information |
|:---------------------------------:|:------:|:-----------------------|
| [001](SPEC.md#ZG-CONFORMANCE-001) |   ✓    |                        |
| [002](SPEC.md#ZG-CONFORMANCE-002) |   ✓    |                        |
| [003](SPEC.md#ZG-CONFORMANCE-003) |   ✓    |                        |
| [004](SPEC.md#ZG-CONFORMANCE-004) |   ✓    |                        |
| [005](SPEC.md#ZG-CONFORMANCE-005) |   ✓    |                        |
| [006](SPEC.md#ZG-CONFORMANCE-006) |   ✓    |                        |
| [007](SPEC.md#ZG-CONFORMANCE-007) |   ✓    |                        |
| [008](SPEC.md#ZG-CONFORMANCE-008) |   ✓    |                        |
| [009](SPEC.md#ZG-CONFORMANCE-009) |   ✓    |                        |
| [010](SPEC.md#ZG-CONFORMANCE-010) |   ✓    |                        |
| [011](SPEC.md#ZG-CONFORMANCE-011) |   ✓    |                        |
| [012](SPEC.md#ZG-CONFORMANCE-012) |   ✓    |                        |
| [013](SPEC.md#ZG-CONFORMANCE-013) |   ✓    |                        |
| [014](SPEC.md#ZG-CONFORMANCE-014) |   ✓    |                        |
| [015](SPEC.md#ZG-CONFORMANCE-015) |   ✓    |                        |
| [016](SPEC.md#ZG-CONFORMANCE-016) |   ✓    |                        |
| [017](SPEC.md#ZG-CONFORMANCE-017) |   ✓    |                        |
| [018](SPEC.md#ZG-CONFORMANCE-018) |   ✓    |                        |
| [019](SPEC.md#ZG-CONFORMANCE-019) |   ✓    |                        |
| [020](SPEC.md#ZG-CONFORMANCE-020) |   ✓    |                        |
| [021](SPEC.md#ZG-CONFORMANCE-021) |   ✓    |                        |
| [022](SPEC.md#ZG-CONFORMANCE-022) |   ✓    |                        |
| [023](SPEC.md#ZG-CONFORMANCE-023) |   ✓    |                        |
| [024](SPEC.md#ZG-CONFORMANCE-024) |   ✓    |                        |
| [025](SPEC.md#ZG-CONFORMANCE-025) |   ✓    |                        |
| [026](SPEC.md#ZG-CONFORMANCE-026) |   ✓    |                        |

### Performance

Tests are ignored by default. To explicitly run performance tests, run the command:
```
 cargo +stable test performance --features performance
```

|             Test Case             | Status | Additional Information |
|:---------------------------------:|:------:|:-----------------------|
| [001](SPEC.md#ZG-PERFORMANCE-001) |   ✓    |                        |
| [002](SPEC.md#ZG-PERFORMANCE-002) |   ✓    |                        |
| [003](SPEC.md#ZG-PERFORMANCE-003) |   ✓    |                        |

### Resistance

|            Test Case             | Status | Additional Information |
|:--------------------------------:|:------:|:-----------------------|
| [001](SPEC.md#ZG-RESISTANCE-001) |   ✓    |                        |
| [002](SPEC.md#ZG-RESISTANCE-002) |  ✓/✖   | ⚠ Fails in rare cases  |
| [003](SPEC.md#ZG-RESISTANCE-003) |   ✓    |                        |
| [004](SPEC.md#ZG-RESISTANCE-004) |   ✓    |                        |
