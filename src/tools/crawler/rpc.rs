use std::{
    fs,
    net::SocketAddr,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use jsonrpsee::{
    server::{ServerBuilder, ServerHandle},
    RpcModule,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use ziggurat_core_crawler::summary::NetworkSummary;

#[derive(Default, Clone, Deserialize, Serialize)]
pub struct DumpSummary {
    /// 'success' or 'fail'.
    pub status: String,
    /// file length written, -1 if error
    pub length: i32,
    /// empty if success; error message if fail
    pub message: String,
}

pub struct RpcContext(Arc<Mutex<NetworkSummary>>);

impl RpcContext {
    /// Creates a new RpcContext.
    pub(crate) fn new(network_summary: Arc<Mutex<NetworkSummary>>) -> RpcContext {
        RpcContext(network_summary)
    }
}

pub async fn initialize_rpc_server(rpc_addr: SocketAddr, rpc_context: RpcContext) -> ServerHandle {
    let server = ServerBuilder::default().build(rpc_addr).await.unwrap();
    let module = create_rpc_module(rpc_context);

    debug!("Starting RPC server at {:?}", server.local_addr().unwrap());
    let server_handle = server.start(module).unwrap();

    debug!("RPC server was successfully started");
    server_handle
}

fn create_rpc_module(rpc_context: RpcContext) -> RpcModule<RpcContext> {
    let mut module = RpcModule::new(rpc_context);
    module
        .register_method("getmetrics", |_params, rpc_context| {
            Ok(rpc_context.0.lock().unwrap().clone())
        })
        .unwrap();
    module
        .register_method("dumpmetrics", |params, rpc_context| {
            let report_params = params.parse::<ReportParams>()?;
            if let Some(path) = report_params.file {
                let content = serde_json::to_string(rpc_context.0.lock().unwrap().deref())?;
                // Wrap our NetworkSummary in a JSON-RPC response envelope
                let response =
                    "{\"jsonrpc\":\"2.0\",\"result\":".to_owned() + &content + ",\"id\":0}";
                let length = response.len() as i32;
                // TODO: consider some checks against directory traversal
                if let Err(e) = fs::write(path, response) {
                    warn!("Unable to write to file: {}", e);
                    Ok(DumpSummary {
                        status: "fail".to_string(),
                        length: -1,
                        message: "Unable to write file: ".to_string() + &e.to_string(),
                    })
                } else {
                    Ok(DumpSummary {
                        status: "success".to_string(),
                        length: length,
                        message: "".to_string(),
                    })
                }
            } else {
                Ok(DumpSummary {
                    status: "fail".to_string(),
                    length: -1,
                    message: "No file parameter in params".to_string(),
                })
            }
        })
        .unwrap();
    module
}

/// Represents how to return [NetworkSummary].
#[derive(Deserialize, Debug)]
pub struct ReportParams {
    /// If present then [NetworkSummary] will be written to given file.
    file: Option<PathBuf>,
}
