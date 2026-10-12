//! HTTP RPC transport that keeps the endpoint URL out of errors.
//!
//! `RPC_URL` carries the provider API key (Alchemy: `/v2/<key>`). reqwest
//! errors print `for url (<full url>)` on timeouts, connection resets and DNS
//! failures, and alloy passes them through unchanged, so every `{e}` logged
//! from a provider call would log the key. Every provider is built on
//! [`rpc_client`], whose errors are stripped of the URL at the source.

use std::task::{Context, Poll};

use alloy::rpc::client::RpcClient;
use alloy::rpc::json_rpc::{RequestPacket, ResponsePacket};
use alloy::transports::http::{Http, reqwest};
use alloy::transports::{RpcError, TransportError, TransportErrorKind, TransportFut};

const UNPARSEABLE: &str = "<unparseable url>";

/// Reduce a URL to its host (and port), dropping userinfo, path, query and
/// fragment, any of which may hold an API key. Parsed like the transport
/// parses it (`\\` is a path separator for http URLs); input without a host
/// becomes a fixed placeholder rather than being echoed.
pub fn redact_url(url: &str) -> String {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return UNPARSEABLE.to_string();
    };
    match (parsed.host_str(), parsed.port()) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.to_string(),
        (None, _) => UNPARSEABLE.to_string(),
    }
}

/// Drop the request URL from a reqwest transport error, keeping its kind
/// (timeout, connect, ...) and source chain.
fn strip_url(err: TransportError) -> TransportError {
    match err {
        RpcError::Transport(TransportErrorKind::Custom(inner)) => {
            match inner.downcast::<reqwest::Error>() {
                Ok(e) => TransportErrorKind::custom(e.without_url()),
                Err(inner) => RpcError::Transport(TransportErrorKind::Custom(inner)),
            }
        }
        other => other,
    }
}

#[derive(Clone)]
struct UrlStrippingHttp(Http<reqwest::Client>);

impl tower::Service<RequestPacket> for UrlStrippingHttp {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx).map_err(strip_url)
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let fut = self.0.call(req);
        Box::pin(async move { fut.await.map_err(strip_url) })
    }
}

/// Build an HTTP RPC client for `url` whose errors never carry the URL.
pub fn rpc_client(url: &str) -> Result<RpcClient, String> {
    let parsed: reqwest::Url = url
        .parse()
        .map_err(|e| format!("Invalid RPC URL for host '{}': {e}", redact_url(url)))?;
    let http = Http::new(parsed);
    let is_local = http.guess_local();
    Ok(RpcClient::new(UrlStrippingHttp(http), is_local))
}

// Named `unit_tests` so the CI filter (`cargo test unit_tests`) runs them.
#[cfg(test)]
mod unit_tests {
    use super::*;
    use alloy::providers::{Provider, ProviderBuilder};
    use tower::Service;

    const KEY: &str = "alch_secretkey123";

    #[test]
    fn redact_url_keeps_only_the_host() {
        assert_eq!(
            redact_url(&format!("https://arb-mainnet.g.alchemy.com/v2/{KEY}")),
            "arb-mainnet.g.alchemy.com"
        );
        assert_eq!(
            redact_url("https://u:pass@host.example:8545/rpc?apikey=k#f"),
            "host.example:8545"
        );
        assert_eq!(redact_url("http://127.0.0.1:8545"), "127.0.0.1:8545");
    }

    #[test]
    fn redact_url_treats_backslash_as_a_path_separator() {
        assert_eq!(
            redact_url(&format!("https://host.example\\v2\\{KEY}")),
            "host.example"
        );
    }

    #[test]
    fn unparseable_urls_never_echo_the_key() {
        for input in [
            format!("https://host:99999\\v2\\{KEY}"),
            format!("not a url/{KEY}"),
            format!("localhost:8545/{KEY}"),
        ] {
            assert_eq!(redact_url(&input), UNPARSEABLE, "{input:?}");
            if let Err(err) = rpc_client(&input) {
                assert!(!err.contains(KEY), "{err}");
            }
        }
    }

    /// A refused connection is the common case where reqwest prints the URL.
    #[tokio::test]
    async fn provider_errors_do_not_carry_the_url() {
        let url = format!("http://127.0.0.1:1/v2/{KEY}");

        let raw = Http::<reqwest::Client>::new(url.parse().unwrap())
            .call(RequestPacket::Batch(vec![]))
            .await
            .unwrap_err();
        assert!(
            raw.to_string().contains(KEY),
            "precondition: unwrapped reqwest error embeds the URL: {raw}"
        );

        let provider = ProviderBuilder::new().connect_client(rpc_client(&url).unwrap());
        let err = provider.get_block_number().await.unwrap_err();
        assert!(!err.to_string().contains(KEY), "{err}");
        assert!(!format!("{err:?}").contains(KEY), "{err:?}");
    }
}
