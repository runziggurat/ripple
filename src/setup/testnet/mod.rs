//! Utilities for setting up a testnet consisting of 3 nodes.

use std::{
    fmt,
    fmt::Write,
    fs, io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use crate::setup::{
    build_ripple_work_path,
    constants::{
        DEFAULT_PORT, STATEFUL_NODES_COUNT, TESTNET_NETWORK_ID, VALIDATORS_FILE_NAME, VALIDATOR_IPS,
    },
    node::{Node, NodeBuilder, NodeType},
};

/// Testnet's directory for nodes' configs.
const TESTNET_DIR: &str = "testnet";

const VALIDATOR_KEYS: [&str; STATEFUL_NODES_COUNT] = [
    "nHUSqn9qjEF7JJkVqvY7BFLMKdqP5KLLEjo5oB4QH43ADDndRawB",
    "nHUEsvSFTf1Snr7ZUdLxjcMW6PKcMrwwXCGZBg6xb1ePG8R4C3TS",
    "nHUuYdS49cPfRmCXPTwu7MVVFZFFmfG7y5sRttirVMhwuD7xStQp",
];

/// Get validator token.
pub fn get_validator_token(stateful_node_idx: usize) -> String {
    match stateful_node_idx {
        0 => include_str!("validator_token0.txt").into(),
        1 => include_str!("validator_token1.txt").into(),
        2 => include_str!("validator_token2.txt").into(),
        _ => panic!("validator token file does not exist"),
    }
}

/// A struct to conveniently start and stop a small testnet.
pub struct TestNet {
    // Setup information for each node. Used for writing configuration.
    pub setups: [NodeSetup; VALIDATOR_KEYS.len()],
    // Running nodes. Used to stop the testnet.
    pub running: Vec<Node>,
    // Sets whether to log the node's output to Ziggurat's output stream.
    use_stdout: bool,
    // Path under which all nodes will be built
    path: PathBuf,
}

impl TestNet {
    /// Creates a new TestNet (without starting it).
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            setups: [
                NodeSetup::new(
                    VALIDATOR_IPS[0].parse().unwrap(),
                    VALIDATOR_KEYS[0].into(),
                    get_validator_token(0),
                ),
                NodeSetup::new(
                    VALIDATOR_IPS[1].parse().unwrap(),
                    VALIDATOR_KEYS[1].into(),
                    get_validator_token(1),
                ),
                NodeSetup::new(
                    VALIDATOR_IPS[2].parse().unwrap(),
                    VALIDATOR_KEYS[2].into(),
                    get_validator_token(2),
                ),
            ],
            running: vec![],
            use_stdout: false,
            path: build_testnet_path()?,
        })
    }

    /// Starts a testnet.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        self.cleanup().await?;
        let validators_contents = self.build_validators_file_contents().await?;

        for (i, setup) in self.setups.iter().enumerate() {
            let node = self
                .start_node(&i.to_string(), setup, &validators_contents)
                .await?;
            self.running.push(node);
        }
        Ok(())
    }

    /// Stops the testnet.
    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.running.iter_mut().for_each(|node| {
            if let Err(e) = node.stop() {
                eprintln!("Unable to stop node: {e:?}");
            }
        });
        Ok(())
    }

    // Creates `validators.txt` file with keys of all nodes.
    async fn build_validators_file_contents(&self) -> Result<String, fmt::Error> {
        let mut config_str = String::new();
        writeln!(&mut config_str, "[validators]")?;
        for n in &self.setups {
            writeln!(&mut config_str, "{}", n.validator_key)?;
        }
        Ok(config_str)
    }

    // Removes ~/.ziggurat/ripple/testnet directory
    async fn cleanup(&self) -> io::Result<()> {
        if let Err(e) = fs::remove_dir_all(&self.path) {
            // Directory may not exist, so we let that error through
            if e.kind() != io::ErrorKind::NotFound {
                return Err(e);
            }
        }
        Ok(())
    }

    // Starts a node in the testnet. `suffix` is used to determine a name for the node's subdirectory.
    async fn start_node(
        &self,
        suffix: &str,
        setup: &NodeSetup,
        validators_contents: &str,
    ) -> anyhow::Result<Node> {
        let target_path = self.path.join(suffix);
        if !target_path.exists() {
            fs::create_dir_all(&target_path)?;
        }

        write_validators_file(&target_path, validators_contents).await?;
        NodeBuilder::stateless()?
            .initial_peers(self.collect_other_peers(setup))
            .set_addr(SocketAddr::new(setup.ip, DEFAULT_PORT))
            .validator_token(setup.validator_token.clone())
            .network_id(TESTNET_NETWORK_ID)
            .log_to_stdout(self.use_stdout)
            .start(&target_path, NodeType::Testnet)
            .await
    }

    // Builds a list of peers for the node. Each node has two peers (the other nodes in the testnet).
    fn collect_other_peers(&self, setup: &NodeSetup) -> Vec<SocketAddr> {
        self.setups
            .iter()
            .filter_map(|peer| {
                if peer.ip != setup.ip {
                    Some(SocketAddr::new(peer.ip, DEFAULT_PORT))
                } else {
                    None
                }
            })
            .collect()
    }
}

// Saves `validators.txt` file in a node's subdirectory.
async fn write_validators_file(path: &Path, contents: &str) -> io::Result<()> {
    let path = path.join(VALIDATORS_FILE_NAME);
    fs::write(path, contents)
}

// Convenience function to build testnet's path.
fn build_testnet_path() -> io::Result<PathBuf> {
    Ok(build_ripple_work_path()?.join(TESTNET_DIR))
}

// Describes each node's setup.
pub struct NodeSetup {
    // The node's ip address.
    ip: IpAddr,
    // The node's validator key to be put in the validators.txt file.
    validator_key: String,
    // The node's validator token to be put in the rippled.cfg file.
    pub validator_token: String,
}

impl NodeSetup {
    fn new(ip: IpAddr, validator_key: String, validator_token: String) -> Self {
        Self {
            ip,
            validator_key,
            validator_token,
        }
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::setup::testnet::TestNet;

    #[ignore = "used to set up a small testnet that can be used to procure node state"]
    #[tokio::test]
    async fn run_testnet() {
        let mut testnet = TestNet::new().unwrap();
        testnet.use_stdout = false;
        testnet.start().await.unwrap();
        // TODO wait for nodes to start and verify state. At the moment the test is successful it it doesn't panic.
        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
        testnet.stop().await.unwrap();
    }
}
