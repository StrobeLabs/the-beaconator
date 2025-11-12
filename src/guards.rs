use crate::models::AppState;
use rocket::{Request, State, http::Status, request::FromRequest, request::Outcome};
use rocket_okapi::{
    r#gen::OpenApiGenerator,
    okapi::openapi3::{Object, SecurityRequirement, SecurityScheme, SecuritySchemeData},
    request::{OpenApiFromRequest, RequestHeaderInput},
};
use sentry;
use tracing;

/// API token guard for request authentication.
///
/// Validates that requests include a valid Bearer token in the Authorization header.
/// The token must match the configured BEACONATOR_ACCESS_TOKEN.
pub struct ApiToken(pub String);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiToken {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let endpoint = request.uri().to_string();

        let state = request.guard::<&State<AppState>>().await;
        match state {
            Outcome::Success(state) => {
                let auth_header = request.headers().get_one("Authorization");
                match auth_header {
                    Some(header) if header.starts_with("Bearer ") => {
                        let token = &header[7..]; // Remove "Bearer " prefix
                        if token == state.access_token {
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

impl<'r> OpenApiFromRequest<'r> for ApiToken {
    fn from_request_input(
        _gen: &mut OpenApiGenerator,
        _name: String,
        _required: bool,
    ) -> rocket_okapi::Result<RequestHeaderInput> {
        // Define Bearer token authentication scheme
        let security_scheme = SecurityScheme {
            description: Some(
                "Bearer token authentication. Include your API token in the Authorization header \
                 as: `Authorization: Bearer YOUR_TOKEN`"
                    .to_string(),
            ),
            data: SecuritySchemeData::Http {
                scheme: "bearer".to_string(),
                bearer_format: Some("API token".to_string()),
            },
            extensions: Object::default(),
        };

        // Create security requirement referencing this scheme
        let mut security_req = SecurityRequirement::new();
        security_req.insert("bearerAuth".to_string(), Vec::new());

        Ok(RequestHeaderInput::Security(
            "bearerAuth".to_string(),
            security_scheme,
            security_req,
        ))
    }
}
