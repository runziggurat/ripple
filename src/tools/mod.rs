//! Utilities for network testing.

pub mod config;
pub mod constants;
// This mod belongs to the tools/crawler and we are using a sym
// link to get it here.
// This is a workaround solution in this repo for this case,
// in future Ziggurat repos, we will handle this differently.
pub mod crawl;
pub mod inner_node;
pub mod ips;
pub mod rpc;
pub mod synth_node;
pub mod tls_cert;

/// Waits until an expression is true or times out.
///
/// Uses polling to cut down on time otherwise used by calling `sleep` in tests.
#[macro_export]
macro_rules! wait_until {
    ($wait_limit: expr, $condition: expr $(, $sleep_duration: expr)?) => {
        let now = std::time::Instant::now();
        loop {
            if $condition {
                break;
            }

            // Default timeout.
            let sleep_duration = std::time::Duration::from_millis(10);
            // Set if present in args.
            $(let sleep_duration = $sleep_duration;)?
            tokio::time::sleep(sleep_duration).await;
            if now.elapsed() > $wait_limit {
                panic!("timed out!");
            }
        }
    };
}
