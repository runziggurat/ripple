## Run performance tests
Create a package of IP addresses which are required for performance tests.

You must first fetch the `ips.py` script from the ziggurat-core repository.  Run this:

```bash
wget -O tools/ips.py https://raw.githubusercontent.com/runziggurat/ziggurat-core/main/ziggurat-core-scripts/ips.py
```

_NOTE: To run the `ips.py` script below, the user must be in the sudoers file in order to use this script.
Script uses `ip`/`ipconfig` commands which require sudo privilages._

From the root repository directory, depending on your OS, run one of the following commands.

### Preconditions under Linux
Generate dummy devices with addresses:
```bash
python3 ./tools/ips.py --subnet 1.1.1.0/24 --file tools/ips_list.json --dev_prefix test_zeth
```

### Preconditions under MacOS
Add the whole subnet to the loopback device - can also be used on Linux (device name - Linux: `lo`, MacOS: `lo0`):
```bash
python3 ./tools/ips.py --subnet 1.1.0.0/24 --file tools/ips_list.json --dev lo0
```

Read ./tools/ips.py for more details.

### Run tests
Run performance tests with the following command:
```bash
cargo +stable t performance --features performance -- --test-threads=1
```

