use crate::models::AppState;
use rocket::{Request, State, http::Status, request::FromRequest, request::Outcome};

// API Token guard
pub struct ApiToken;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiToken {
    type Error = String;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let state = request.guard::<&State<AppState>>().await;
        match state {
            Outcome::Success(state) => {
                let auth_header = request.headers().get_one("Authorization");
                match auth_header {
                    Some(header) if header.starts_with("Bearer ") => {
                        let token = &header[7..]; // Remove "Bearer " prefix
                        if token == state.access_token {
                            Outcome::Success(ApiToken)
                        } else {
                            Outcome::Error((Status::Unauthorized, "Invalid API token".to_string()))
                        }
                    }
                    _ => Outcome::Error((
                        Status::Unauthorized,
                        "Missing or invalid Authorization header".to_string(),
                    )),
                }
            }
            _ => Outcome::Error((
                Status::InternalServerError,
                "Application state not available".to_string(),
            )),
        }
    }
}
