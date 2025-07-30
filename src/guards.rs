use crate::models::AppState;
use rocket::{Request, State, http::Status, request::FromRequest, request::Outcome};
use sentry;
use tracing;

// API Token guard
pub struct ApiToken(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiToken {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let endpoint = request.uri().to_string();
        tracing::debug!("ApiToken guard checking authentication for: {}", endpoint);

        let state = request.guard::<&State<AppState>>().await;
        match state {
            Outcome::Success(state) => {
                let auth_header = request.headers().get_one("Authorization");
                match auth_header {
                    Some(header) if header.starts_with("Bearer ") => {
                        let token = &header[7..]; // Remove "Bearer " prefix
                        if token == state.access_token {
                            tracing::debug!("Authentication successful for: {}", endpoint);
                            Outcome::Success(ApiToken(token.to_string()))
                        } else {
                            tracing::warn!("Invalid API token provided for: {}", endpoint);
                            sentry::capture_message(
                                &format!("Invalid API token attempt for: {endpoint}"),
                                sentry::Level::Warning,
                            );
                            Outcome::Error((Status::Unauthorized, "Invalid API token".to_string()))
                        }
                    }
                    Some(_header) => {
                        tracing::warn!(
                            "Authorization header doesn't start with 'Bearer ' for: {}",
                            endpoint
                        );
                        Outcome::Error((
                            Status::Unauthorized,
                            "Authorization header must start with 'Bearer '".to_string(),
                        ))
                    }
                    None => {
                        tracing::warn!("Missing Authorization header for: {}", endpoint);
                        Outcome::Error((
                            Status::Unauthorized,
                            "Missing Authorization header".to_string(),
                        ))
                    }
                }
            }
            _ => {
                tracing::error!("Application state not available for: {}", endpoint);
                sentry::capture_message(
                    "Application state not available in ApiToken guard",
                    sentry::Level::Error,
                );
                Outcome::Error((
                    Status::InternalServerError,
                    "Application state not available".to_string(),
                ))
            }
        }
    }
}
