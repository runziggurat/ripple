use std::{
    collections::HashSet,
    fs, io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use anyhow::Result;
use fs_extra::{dir, file};
use tokio::{io::AsyncWriteExt, net::TcpStream, time::Duration};

use crate::setup::{
    build_ripple_work_path,
    config::{NodeMetaData, RippledConfigFile},
    constants::{
        CONNECTION_TIMEOUT, DEFAULT_PORT, JSON_RPC_PORT, RIPPLED_CONFIG, RIPPLE_SETUP_DIR,
        STATEFUL_NODES_COUNT, STATEFUL_NODES_DIR, TESTNET_NETWORK_ID, VALIDATORS_FILE_NAME,
        VALIDATOR_IPS,
    },
    testnet::get_validator_token,
};

async fn wait_for_start(addr: SocketAddr) {
    tokio::time::timeout(CONNECTION_TIMEOUT, async {
        const SLEEP: Duration = Duration::from_millis(10);

        loop {
            if let Ok(mut stream) = TcpStream::connect(addr).await {
                stream.shutdown().await.unwrap();
                break;
            }

            tokio::time::sleep(SLEEP).await;
        }
    })
    .await
    .unwrap();
}

pub enum ChildExitCode {
    Success,
    ErrorCode(Option<i32>),
}

/// Node type is used to select different startup configurations.
pub enum NodeType {
    /// A temporary node used to store ledger data for stateful nodes. Should not be used otherwise.
    Testnet,
    /// A non-validator node without any preloaded data.
    Stateless,
    /// A validator node with a preloaded ledger data.
    Stateful,
}

pub struct NodeBuilder {
    /// Node's startup configuration.
    conf: NodeConfig,
    /// Node's process metadata read from Ziggurat configuration files.
    meta: NodeMetaData,
    /// Counter for served stateful nodes.
    stateful_nodes_counter: usize,
}

impl NodeBuilder {
    /// Creates new [NodeBuilder] which can handle stateless nodes.
    pub fn stateless() -> anyhow::Result<Self> {
        let setup_path = build_ripple_work_path()?.join(RIPPLE_SETUP_DIR);

        let conf = NodeConfig::default();
        let meta = NodeMetaData::new(setup_path)?;

        Ok(Self {
            conf,
            meta,
            stateful_nodes_counter: 0,
        })
    }

    /// Creates new [NodeBuilder] which can handle stateful nodes.
    pub fn stateful() -> anyhow::Result<Self> {
        Ok(Self::stateless()
            .expect("Failed to create a node builder")
            .network_id(TESTNET_NETWORK_ID))
    }

    /// Creates [Node] according to configuration and starts its process.
    pub async fn start(&mut self, target: &Path, node_type: NodeType) -> Result<Node> {
        if !target.exists() {
            fs::create_dir_all(&target)?;
        }

        let setup_path = build_ripple_work_path()?.join(RIPPLE_SETUP_DIR);

        match node_type {
            NodeType::Stateful => {
                let node_idx = self.stateful_nodes_counter;
                self.stateful_nodes_counter += 1;
                assert!(
                    self.stateful_nodes_counter <= STATEFUL_NODES_COUNT,
                    "Not enough stateful nodes available"
                );

                let source = get_stateful_node_path(node_idx)?;

                let mut copy_options = dir::CopyOptions::new();
                copy_options.content_only = true;
                copy_options.overwrite = true;
                dir::copy(&source, &target, &copy_options)?;

                self.conf.local_addr =
                    SocketAddr::new(VALIDATOR_IPS[node_idx].parse().unwrap(), DEFAULT_PORT);
                self.conf.validator_token = Some(get_validator_token(node_idx));
                self.meta.start_args = vec![
                    "--valid".into(),
                    "--quorum".into(),
                    "1".into(),
                    "--load".into(),
                ];
            }
            NodeType::Stateless => {
                let validators_file_src = setup_path.join(VALIDATORS_FILE_NAME);
                let validators_file_dst = target.join(VALIDATORS_FILE_NAME);

                let copy_options = file::CopyOptions::new();
                file::copy(&validators_file_src, &validators_file_dst, &copy_options)?;

                self.conf.network_id = None;
                self.conf.validator_token = None;
                self.conf.local_addr =
                    SocketAddr::new(VALIDATOR_IPS[0].parse().unwrap(), DEFAULT_PORT);
            }
            NodeType::Testnet => (),
        }

        let rippled_cfg = RippledConfigFile::generate(&self.conf, target)?;
        let rippled_cfg_path = target.join(RIPPLED_CONFIG);
        fs::write(rippled_cfg_path.clone(), rippled_cfg)?;

        self.meta.start_args.push("--conf".into());
        self.meta.start_args.push(rippled_cfg_path.into());

        let node = self.start_node();
        wait_for_start(node.config.local_addr).await;

        self.meta = NodeMetaData::new(setup_path)?; // Reset args
        Ok(node)
    }

    /// Sets address to bind to.
    pub fn set_addr(mut self, addr: SocketAddr) -> Self {
        self.conf.local_addr = addr;
        self
    }

    /// Sets initial peers for the node.
    pub fn initial_peers(mut self, addrs: Vec<SocketAddr>) -> Self {
        self.conf.initial_peers = addrs.into_iter().collect();
        self
    }

    /// Sets validator token to be placed in rippled.cfg.
    /// This will configure the node to run as a validator.
    pub fn validator_token(mut self, token: String) -> Self {
        self.conf.validator_token = Some(token);
        self
    }

    /// Sets network's id to form an isolated testnet.
    pub fn network_id(mut self, network_id: u32) -> Self {
        self.conf.network_id = Some(network_id);
        self
    }

    /// Sets whether to log the node's output to Ziggurat's output stream.
    pub fn log_to_stdout(mut self, log_to_stdout: bool) -> Self {
        self.conf.log_to_stdout = log_to_stdout;
        self
    }

    fn start_node(&self) -> Node {
        let (stdout, stderr) = match self.conf.log_to_stdout {
            true => (Stdio::inherit(), Stdio::inherit()),
            false => (Stdio::null(), Stdio::null()),
        };

        let child = Command::new(&self.meta.start_command)
            .current_dir(&self.meta.path)
            .args(&self.meta.start_args)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .expect("node failed to start");

        Node {
            child,
            meta: self.meta.clone(),
            config: self.conf.clone(),
        }
    }
}

/// Startup configuration for the node.
/// Some fields are written to the node's configuration file.
#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// The socket address of the node.
    pub local_addr: SocketAddr,
    /// The initial peer set of the node.
    pub initial_peers: HashSet<SocketAddr>,
    /// The initial max number of peer connections to allow.
    pub max_peers: usize,
    /// Token when run as a validator.
    pub validator_token: Option<String>,
    /// Network's id to form an isolated testnet.
    pub network_id: Option<u32>,
    /// Setting this option to true will enable node logging to stdout.
    pub log_to_stdout: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            local_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, DEFAULT_PORT)),
            initial_peers: Default::default(),
            max_peers: 0,
            validator_token: None,
            network_id: None,
            log_to_stdout: false,
        }
    }
}

pub struct Node {
    child: Child,
    config: NodeConfig,
    #[allow(dead_code)]
    meta: NodeMetaData,
}

impl Node {
    pub fn builder() -> NodeBuilder {
        NodeBuilder::stateful()
            .map_err(|e| format!("Unable to create builder: {:?}", e))
            .unwrap()
    }

    pub fn stop(&mut self) -> io::Result<ChildExitCode> {
        match self.child.try_wait()? {
            None => self.child.kill()?,
            Some(code) => return Ok(ChildExitCode::ErrorCode(code.code())),
        }
        let exit = self.child.wait()?;

        match exit.code() {
            None => Ok(ChildExitCode::Success),
            Some(exit) if exit == 0 => Ok(ChildExitCode::Success),
            Some(exit) => Ok(ChildExitCode::ErrorCode(Some(exit))),
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.config.local_addr
    }

    pub fn rpc_url(&self) -> String {
        format!(
            "http://{addr}:{port}",
            addr = self.config.local_addr.ip(),
            port = JSON_RPC_PORT
        )
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        // We should avoid a panic.
        if let Err(e) = self.stop() {
            eprintln!("Failed to stop the node: {}", e);
        }
    }
}

fn get_stateful_node_path(node_dir: usize) -> io::Result<PathBuf> {
    let ziggurat_path = build_ripple_work_path()?;
    Ok(ziggurat_path
        .join(STATEFUL_NODES_DIR)
        .join(node_dir.to_string()))
}

#[cfg(test)]
mod test {
    use tempfile::TempDir;
    use tokio::time::sleep;

    use super::*;

    const STATELESS_NODE_CNT: usize = 3; // Any number should work

    const SLEEP: Duration = Duration::from_millis(100);

    #[tokio::test]
    #[ignore = "use only when changing src/setup files"]
    async fn run_stateless_nodes_in_parallel() {
        let mut builder = NodeBuilder::stateless().expect("Can't build a stateless node");
        let mut nodes = Vec::<Node>::with_capacity(STATELESS_NODE_CNT);

        for _ in 0..STATELESS_NODE_CNT {
            let target = TempDir::new().expect("Can't build tmp dir");

            let node = builder
                .start(target.path(), NodeType::Stateless)
                .await
                .expect("Unable to start node");
            nodes.push(node);
        }

        sleep(SLEEP).await;

        for mut node in nodes {
            node.stop().unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "use only when changing src/setup files"]
    async fn run_stateless_nodes_sequentially() {
        let mut builder = NodeBuilder::stateless().expect("Can't build a stateless node");

        for _ in 0..STATELESS_NODE_CNT {
            let target = TempDir::new().expect("Can't build tmp dir");

            let mut node = builder
                .start(target.path(), NodeType::Stateless)
                .await
                .expect("Unable to start node");

            sleep(SLEEP).await;
            node.stop().unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "use only when changing src/setup files"]
    async fn run_stateful_nodes_sequentially() {
        let mut builder = NodeBuilder::stateful().expect("Can't build a stateful node");

        for _ in 0..STATEFUL_NODES_COUNT {
            let target = TempDir::new().expect("Can't build tmp dir");

            let mut node = builder
                .start(target.path(), NodeType::Stateful)
                .await
                .expect("Unable to start node");

            sleep(SLEEP).await;
            node.stop().unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "use only when changing src/setup files"]
    #[should_panic]
    async fn run_too_many_stateful_nodes_sequentially() {
        let mut builder = NodeBuilder::stateful().expect("Can't build a stateful node");

        for _ in 0..STATEFUL_NODES_COUNT + 1 {
            let target = TempDir::new().expect("Can't build tmp dir");
            let mut node = builder
                .start(target.path(), NodeType::Stateful)
                .await
                .expect("Unable to start node");

            sleep(SLEEP).await;
            node.stop().unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "use only when changing src/setup files"]
    async fn run_stateful_nodes_in_parallel() {
        let mut builder = NodeBuilder::stateful().expect("Can't build a stateful node");
        let mut nodes = Vec::<Node>::with_capacity(STATEFUL_NODES_COUNT);

        for _ in 0..STATEFUL_NODES_COUNT {
            let target = TempDir::new().expect("Can't build tmp dir");

            let node = builder
                .start(target.path(), NodeType::Stateful)
                .await
                .expect("Unable to start node");
            nodes.push(node);
        }

        sleep(SLEEP).await;

        for mut node in nodes {
            node.stop().unwrap();
        }
    }
}
