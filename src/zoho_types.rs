//! Module containing Zoho API-specific types.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug)]
pub struct AccessTokenRequest {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub grant_type: String,
}

#[derive(Deserialize, Debug)]
pub struct AccessTokenResponseInner {
    pub access_token: String,
    #[allow(dead_code)]
    pub expires_in: u32,
    #[allow(dead_code)]
    pub api_domain: String,
    #[allow(dead_code)]
    pub token_type: String,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum AccessTokenResponse {
    Success(AccessTokenResponseInner),
    Error(ApiError),
}

#[derive(Deserialize, Debug)]
pub struct ApiError {
    pub error: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// AccessTokenRequest should serialize to form-compatible fields.
    fn access_token_request_serializes() {
        let req = AccessTokenRequest {
            client_id: "cid123".to_string(),
            client_secret: "csecret".to_string(),
            refresh_token: "rtoken".to_string(),
            grant_type: "refresh_token".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("cid123"));
        assert!(json.contains("csecret"));
        assert!(json.contains("rtoken"));
        assert!(json.contains("refresh_token"));
    }

    #[test]
    /// A successful access token response should deserialize into Success variant.
    fn access_token_response_success_deserializes() {
        let json = r#"{
            "access_token": "tok_abc123",
            "expires_in": 3600,
            "api_domain": "https://www.zohoapis.com",
            "token_type": "Bearer"
        }"#;
        let resp: AccessTokenResponse = serde_json::from_str(json).unwrap();
        assert!(matches!(resp, AccessTokenResponse::Success(_)));
        if let AccessTokenResponse::Success(inner) = resp {
            assert_eq!(inner.access_token, "tok_abc123");
            assert_eq!(inner.expires_in, 3600);
            assert_eq!(inner.token_type, "Bearer");
        }
    }

    #[test]
    /// An error access token response should deserialize into Error variant.
    fn access_token_response_error_deserializes() {
        let json = r#"{"error": "invalid_client"}"#;
        let resp: AccessTokenResponse = serde_json::from_str(json).unwrap();
        assert!(matches!(resp, AccessTokenResponse::Error(_)));
        if let AccessTokenResponse::Error(e) = resp {
            assert_eq!(e.error, "invalid_client");
        }
    }

    #[test]
    /// ApiError should deserialize its error field correctly.
    fn api_error_deserializes() {
        let json = r#"{"error": "access_denied"}"#;
        let err: ApiError = serde_json::from_str(json).unwrap();
        assert_eq!(err.error, "access_denied");
    }
}
