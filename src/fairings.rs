use rocket::fairing::{Fairing, Info, Kind};
use rocket::{Data, Request, Response};

/// Logs incoming requests and outgoing responses.
///
/// Captures method, URI, remote address, and response status for monitoring and debugging.
pub struct RequestLogger;

#[rocket::async_trait]
impl Fairing for RequestLogger {
    fn info(&self) -> Info {
        Info {
            name: "Request/Response Logger",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_request(&self, request: &mut Request<'_>, _: &mut Data<'_>) {
        // ECS / ALB health checks hit /health every few seconds; don't log them.
        if request.uri().path() == "/health" {
            return;
        }

        let method = request.method();
        let uri = request.uri();
        let remote = request
            .remote()
            .map(|r| r.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        tracing::info!("Incoming request: {} {} from {}", method, uri, remote);

        // Log authentication header presence only
        if request.headers().get_one("authorization").is_some() {
            tracing::trace!("Request includes authorization header");
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        // ECS / ALB health checks hit /health every few seconds; don't log them.
        if request.uri().path() == "/health" {
            return;
        }

        let method = request.method();
        let uri = request.uri();
        let status = response.status();

        // Log the response
        tracing::info!("Response: {} {} - Status: {}", method, uri, status);

        // If it's an error, log more details
        if !status.class().is_success() {
            tracing::error!("Error response: {} {} returned {}", method, uri, status);
        }
    }
}

/// Catches and logs internal server errors that may indicate panics.
///
/// Response-side hook kept for symmetry; 500 logging lives in lib.rs's catchers.
pub struct PanicCatcher;

#[rocket::async_trait]
impl Fairing for PanicCatcher {
    fn info(&self) -> Info {
        Info {
            name: "Panic Catcher",
            kind: Kind::Request | Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, _response: &mut Response<'r>) {
        // No logging here: the dedicated 500 catchers in lib.rs already emit
        // the request context, and logging from both double-counted every 500
        // in log-based error metrics.
    }
}
