//! Rest API interface for Snowflake
//!

use crate::{consts, creds::SnowflakeCredentials};

use anyhow::Result;
use jsonwebtoken::{encode, get_current_timestamp, Algorithm, EncodingKey, Header};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware, RequestBuilder};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

/// Claims for use with the `jsonwebtoken` crate when
/// creating a new JWT.
#[derive(Debug, Serialize, Deserialize)]
struct JwtClaims {
    /// Required (validate_exp defaults to true in validation). Expiration time (as UTC timestamp)
    exp: usize,
    /// Optional. Issued at (as UTC timestamp)
    iat: usize,
    /// Optional. Issuer
    iss: String,
    /// Optional. Subject (whom token refers to)
    sub: String,
}

/// Wrapper struct for http functionality
pub(crate) struct SnowflakeRestClient {
    /// The credentials used to authenticate into Snowflake.
    credentials: SnowflakeCredentials,
    http_client: ClientWithMiddleware,
}

impl SnowflakeRestClient {
    pub(crate) fn new(credentials: SnowflakeCredentials) -> Self {
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
        let client = ClientBuilder::new(reqwest::Client::new())
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();
        Self {
            credentials,
            http_client: client,
        }
    }
    /// Execute a query, dropping the result.
    ///
    /// `execute` should only be used for
    /// SQL statements that don't expect results,
    /// such as those that are used to update
    /// state in Snowflake.
    pub(crate) async fn execute(&self, sql: &str) -> Result<()> {
        let request = self.get_request(sql)?;
        request.send().await?.text().await?;
        Ok(())
    }

    pub(crate) async fn query(&self, sql: &str) -> Result<String> {
        let request = self.get_request(sql)?;

        let res = request.send().await?.text().await?;
        Ok(res)
    }

    /// If the URL is explicitly defined, that's used first.
    /// Otherwise, the standard account configuration
    /// is used
    fn get_url(&self) -> String {
        let default_url = self.credentials.url.to_owned().unwrap_or_else(|| {
            format![
                "https://{}.snowflakecomputing.com/api/v2/statements",
                self.credentials.account
            ]
        });
        #[cfg(not(test))]
        return default_url;
        #[cfg(test)]
        return match crate::rest::tests::MOCK_HTTP_SERVER.read().unwrap().server {
            Some(ref v) => v.uri(),
            None => default_url,
        };
    }

    fn get_request(&self, sql: &str) -> Result<RequestBuilder> {
        let token = self.get_jwt()?;
        let body = self.get_body(sql);

        Ok(self
            .http_client
            .post(self.get_url())
            .json(&body)
            .header(consts::AUTH_HEADER, format!["Bearer {}", token])
            .header(consts::CONTENT_TYPE_HEADER, "application/json")
            .header(consts::ACCEPT_HEADER, "application/json")
            .header(consts::SNOWFLAKE_AUTH_HEADER, "KEYPAIR_JWT")
            .header(consts::USER_AGENT_HEADER, "jetty-labs"))
    }

    fn get_body<'a>(&'a self, sql: &'a str) -> HashMap<&str, &'a str> {
        let mut body = HashMap::new();
        body.insert("statement", sql);
        body.insert("warehouse", "main");
        body.insert("role", &self.credentials.role);
        body
    }

    fn get_jwt(&self) -> Result<String> {
        let qualified_username = format![
            "{}.{}",
            self.credentials.account.to_uppercase(),
            self.credentials.user.to_uppercase()
        ];

        // Generate jwt
        let claims = JwtClaims {
            exp: (get_current_timestamp() + 3600) as usize,
            iat: get_current_timestamp() as usize,
            iss: format!["{}.{}", qualified_username, self.credentials.public_key_fp],
            sub: qualified_username,
        };

        // println!("{}", self.credentials.private_key.replace(r" ", ""));

        encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(
                self.credentials
                    .private_key
                    .replace(' ', "")
                    .replace("ENDPRIVATEKEY", "END PRIVATE KEY")
                    .replace("BEGINPRIVATEKEY", "BEGIN PRIVATE KEY")
                    .as_bytes(),
            )?,
        )
        .map_err(anyhow::Error::from)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::RwLock;

    use super::*;

    use lazy_static::lazy_static;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    pub struct WiremockServer {
        pub server: Option<MockServer>,
    }

    impl WiremockServer {
        pub fn new() -> Self {
            Self { server: None }
        }

        pub async fn init(&mut self) {
            let mock_server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v2/statements"))
                .respond_with(
                    ResponseTemplate::new(404).set_body_string(r#"{"text": "wiremock cat fact"}"#),
                )
                .mount(&mock_server)
                .await;
            self.server = Some(mock_server);
        }
    }

    lazy_static! {
        pub static ref MOCK_HTTP_SERVER: RwLock<WiremockServer> =
            RwLock::new(WiremockServer::new());
    }

    async fn setup_wiremock() {
        MOCK_HTTP_SERVER.write().unwrap().init().await;
    }

    #[tokio::test]
    async fn test_me() {
        setup_wiremock().await;
        // SnowflakeRestClient {}
    }
}
