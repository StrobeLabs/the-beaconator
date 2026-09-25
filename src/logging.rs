//! Process log subscriber.
//!
//! alloy's HTTP transport opens a DEBUG `ReqwestTransport` span whose `url`
//! field is the full RPC URL, API key included, and the fmt formatter prints
//! span fields on every event inside it. `RUST_LOG` must not be able to turn
//! that on, including through a more specific directive such as
//! `alloy_transport_http::reqwest_transport=debug`, which would override a
//! directive added to the same `EnvFilter`. So the cap is a separate global
//! `Targets` layer that `RUST_LOG` cannot touch.

use tracing::Subscriber;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::filter::{EnvFilter, Targets};
use tracing_subscriber::fmt;
use tracing_subscriber::prelude::*;

pub const DEFAULT_LOG_FILTER: &str = "info,the_beaconator=info,rocket=warn";

/// Targets never logged below INFO, whatever `RUST_LOG` says.
fn hard_caps() -> Targets {
    Targets::new()
        .with_default(LevelFilter::TRACE)
        .with_target("alloy_transport_http", LevelFilter::INFO)
}

/// The filter from `spec` (normally `RUST_LOG`), or the default when it is
/// unset or does not parse.
pub fn env_filter(spec: Option<&str>) -> EnvFilter {
    spec.and_then(|s| EnvFilter::try_new(s).ok())
        .unwrap_or_else(|| EnvFilter::new(DEFAULT_LOG_FILTER))
}

/// The process subscriber: fmt output filtered by `spec`, under the hard caps.
pub fn subscriber(spec: Option<&str>) -> impl Subscriber + Send + Sync {
    tracing_subscriber::registry().with(hard_caps()).with(
        fmt::layer()
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_filter(env_filter(spec)),
    )
}

// Named `unit_tests` so the CI filter (`cargo test unit_tests`) runs them.
#[cfg(test)]
mod unit_tests {
    use super::*;
    use tracing::Level;

    const ALLOY_SPAN_TARGET: &str = "alloy_transport_http::reqwest_transport";

    fn with_subscriber<T>(spec: Option<&str>, f: impl FnOnce() -> T) -> T {
        tracing::subscriber::with_default(subscriber(spec), f)
    }

    #[test]
    fn rust_log_cannot_enable_alloys_url_bearing_span() {
        for spec in [
            Some("debug"),
            Some("trace"),
            Some("alloy_transport_http=trace"),
            Some("alloy_transport_http::reqwest_transport=debug"),
            Some("alloy_transport_http::reqwest_transport[ReqwestTransport]=trace"),
            None,
        ] {
            with_subscriber(spec, || {
                assert!(
                    !tracing::enabled!(target: ALLOY_SPAN_TARGET, Level::DEBUG),
                    "{spec:?} enables alloy's URL-bearing span"
                );
                assert!(
                    tracing::enabled!(target: ALLOY_SPAN_TARGET, Level::WARN),
                    "{spec:?} silences alloy transport warnings"
                );
            });
        }
    }

    #[test]
    fn rust_log_still_controls_everything_else() {
        with_subscriber(Some("debug"), || {
            assert!(tracing::enabled!(target: "the_beaconator", Level::DEBUG));
        });
        with_subscriber(None, || {
            assert!(tracing::enabled!(target: "the_beaconator", Level::INFO));
        });
    }
}
