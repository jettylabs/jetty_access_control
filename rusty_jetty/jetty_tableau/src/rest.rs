use crate::nodes::CreateNode;

use super::*;
use anyhow::{bail, Context};
use jetty_core::connectors::nodes as jetty_nodes;
use reqwest;

/// Wrapper struct for http functionality
#[derive(Default)]
pub(crate) struct TableauRestClient {
    /// The credentials used to authenticate into Snowflake.
    credentials: TableauCredentials,
    http_client: reqwest::Client,
    token: Option<String>,
    site_id: Option<String>,
    api_version: String,
}

impl TableauRestClient {
    pub fn new(credentials: TableauCredentials) -> Self {
        TableauRestClient {
            credentials,
            http_client: reqwest::Client::new(),
            token: None,
            site_id: None,
            api_version: "3.4".to_owned(),
        }
    }

    /// Get site_id token from the TableauRestClient.
    /// If not available, fetch it.
    async fn get_site_id(&mut self) -> Result<String> {
        if let Some(t) = &self.site_id {
            return Ok(t.to_owned());
        };
        self.fetch_token_and_site_id().await?;

        self.site_id
            .to_owned()
            .ok_or(anyhow!("unable to get site id"))
    }

    /// Get authentication token from the TableauRestClient.
    /// If not available, fetch a new token.
    async fn get_token(&mut self) -> Result<String> {
        if let Some(t) = &self.token {
            return Ok(t.to_owned());
        };
        self.fetch_token_and_site_id().await?;

        self.token.to_owned().ok_or(anyhow!("unable to get token"))
    }

    async fn fetch_token_and_site_id(&mut self) -> Result<()> {
        // Set up the request body to get a request token
        let request_body = json!({
            "credentials": {
                "name" : &self.credentials.username,
                "password": &self.credentials.password,
                "site": {
                    "contentUrl": &self.credentials.site_name,
                }
            }
        });
        let resp = self
            .http_client
            .post(format![
                "https://{}/api/{}/auth/signin",
                &self.credentials.server_name, &self.api_version
            ])
            .json(&request_body)
            .header("Accept".to_owned(), "application/json".to_owned())
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let token = resp
            .get("credentials")
            .ok_or(anyhow!["unable to get token from response"])?
            .get("token")
            .ok_or(anyhow!["unable to get token from response"])?
            .as_str()
            .ok_or(anyhow!["unable to get token from response"])?
            .to_string();
        self.token = Some(token.to_owned());

        let site_id = resp
            .get("credentials")
            .ok_or(anyhow!["unable to get site id from response"])?
            .get("site")
            .ok_or(anyhow!["unable to get site id from response"])?
            .get("id")
            .ok_or(anyhow!["unable to get site id from response"])?
            .as_str()
            .ok_or(anyhow!["unable to get site id from response"])?
            .to_string();
        self.site_id = Some(site_id.to_owned());
        Ok(())
    }

    #[allow(dead_code)]
    async fn get_users(&mut self) -> Result<Vec<jetty_nodes::User>> {
        let users = self
            .get_json_response(
                "users".to_owned(),
                None,
                reqwest::Method::GET,
                Some(vec!["users".to_owned(), "user".to_owned()]),
            )
            .await
            .context("fetching users")?;
        users.to_users()
    }

    #[allow(dead_code)]
    async fn get_assets(&mut self) -> Result<Vec<jetty_nodes::Asset>> {
        let todo = "implement for additional asset types";

        let projects = self
            .get_json_response(
                "projects".to_owned(),
                None,
                reqwest::Method::GET,
                Some(vec!["projects".to_owned(), "project".to_owned()]),
            )
            .await
            .context("fetching projects")?;
        projects.to_projects()
    }

    #[allow(dead_code)]
    async fn get_groups(&mut self) -> Result<Vec<jetty_nodes::Group>> {
        let groups = self
            .get_json_response(
                "groups".to_owned(),
                None,
                reqwest::Method::GET,
                Some(vec!["groups".to_owned(), "group".to_owned()]),
            )
            .await
            .context("fetching groups")?;
        let mut groups = groups.to_groups().context("parse JSON into groups")?;

        // get members of the groups
        for i in 0..groups.len() {
            let group_id = groups[i]
                .metadata
                .get("group_id")
                .ok_or(anyhow!("Unable to get group id for {:#?}", groups[i]))?;
            let resp = self
                .get_json_response(
                    format!("groups/{}/users", group_id),
                    None,
                    reqwest::Method::GET,
                    Some(vec!["users".to_owned(), "user".to_owned()]),
                )
                .await
                .context(format!("getting users for group {}", groups[i].name))?;
            groups[i].includes_users = resp.to_users()?.iter().map(|u| u.name.to_owned()).collect();
        }

        for group in &mut groups {
            let group_id = group
                .metadata
                .get("group_id")
                .ok_or(anyhow!("Unable to get group id for {:#?}", group))?;
            let resp = self
                .get_json_response(
                    format!("groups/{}/users", group_id),
                    None,
                    reqwest::Method::GET,
                    Some(vec!["users".to_owned(), "user".to_owned()]),
                )
                .await
                .context(format!("getting users for group {}", group.name))?;
            group.includes_users = resp.to_users()?.iter().map(|u| u.name.to_owned()).collect();
        }
        Ok(groups)
    }

    async fn get_json_response(
        &mut self,
        endpoint: String,
        body: Option<serde_json::Value>,
        method: reqwest::Method,
        path_to_paginated_iterable: Option<Vec<String>>,
    ) -> Result<serde_json::Value> {
        let req = self
            .build_request(endpoint, body, method)
            .await
            .context("building request")?;

        let resp = req
            .try_clone()
            .ok_or(anyhow!("unable to clone request"))?
            .send()
            .await
            .context("making request")?;

        let parsed_response = resp
            .json::<serde_json::Value>()
            .await
            .context("parsing json response")?;

        // Check for pagination
        if let Some(v) = parsed_response.get("pagination") {
            #[derive(Deserialize)]
            struct PaginationInfo {
                #[serde(rename = "pageSize")]
                page_size: String,
                #[serde(rename = "totalAvailable")]
                total_available: String,
            }
            let info: PaginationInfo =
                serde_json::from_value(v.to_owned()).context("parsing pagination information")?;

            let (page_size, total_available) = (
                info.page_size.parse::<usize>()?,
                info.total_available.parse::<usize>()?,
            );

            // Only need to paginate if there are more results than shown on the first page
            let path_to_paginated_iterable = &path_to_paginated_iterable.ok_or(anyhow![
                "cannot use paginated results without path_to_paginated_iterable"
            ])?;

            let extra_page = if total_available % page_size == 0 {
                0
            } else {
                1
            };
            let total_required_pages = total_available / page_size + extra_page;

            let mut results_vec = vec![];

            // get first page of results
            if let serde_json::Value::Array(vals) =
                get_json_from_path(&parsed_response, path_to_paginated_iterable)
                    .context("getting target json object")?
            {
                results_vec.extend(vals);
            } else {
                bail!["Unable to find target array"];
            };

            for page_number in 2..total_required_pages + 1 {
                let paged_resp = req
                    .try_clone()
                    .ok_or(anyhow!("unable to clone request"))?
                    // add a page number to the request
                    .query(&[("pageNumber", page_number.to_string())])
                    .send()
                    .await
                    .context("making request")?
                    .json::<serde_json::Value>()
                    .await
                    .context("parsing json response")?;

                // get each additional page of results
                if let serde_json::Value::Array(vals) =
                    get_json_from_path(&paged_resp, path_to_paginated_iterable)
                        .context("getting target json object")?
                {
                    results_vec.extend(vals);
                } else {
                    return Err(anyhow!["Unable to find target array"]);
                };
            }
            Ok(serde_json::Value::Array(results_vec))
        } else {
            Ok(parsed_response)
        }
    }

    async fn build_request(
        &mut self,
        endpoint: String,
        body: Option<serde_json::Value>,
        method: reqwest::Method,
    ) -> Result<reqwest::RequestBuilder> {
        let request_url = format![
            "https://{}/api/{}/sites/{}/{}",
            self.credentials.server_name.to_owned(),
            self.api_version.to_owned(),
            self.get_site_id().await?,
            endpoint,
        ];

        let mut req = self.http_client.request(method, request_url);
        req = self
            .add_auth(req)
            .await
            .context("adding auth header")?
            .header("Accept", "application/json")
            // In the case that pageSize is allowed, set it to the max
            .query(&[("pageSize", "1000")]);

        // Add body if exists
        if let Some(b) = body {
            req = req.json(&b);
        }

        Ok(req)
    }

    /// Add authentication header to requests
    async fn add_auth(&mut self, req: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {
        let token = self.get_token().await.context("getting token")?;
        let req = req.header("X-Tableau-Auth", token);
        Ok(req)
    }
}

fn get_json_from_path(val: &serde_json::Value, path: &Vec<String>) -> Result<serde_json::Value> {
    let mut full_path: String = "Object".to_owned();
    let mut return_val = val;

    for p in path {
        full_path = format!("{}.{}", full_path, p);
        return_val = return_val.get(p).ok_or(anyhow!(
            "unable to parse json - no such path exists: {}\n{}",
            full_path,
            val
        ))?;
    }
    Ok(return_val.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Context;
    use jetty_core::jetty;

    #[tokio::test]
    async fn test_fetching_token_works() -> Result<()> {
        let mut tc = connector_setup().context("running tableau connector setup")?;
        tc.client.get_token().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_users_works() -> Result<()> {
        let mut tc = connector_setup().context("running tableau connector setup")?;
        let users = tc.client.get_users().await?;
        for u in users {
            println!("{}", u.name);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_assets_works() -> Result<()> {
        let mut tc = connector_setup().context("running tableau connector setup")?;
        let assets = tc.client.get_assets().await?;
        for a in assets {
            println!("{:#?}", a);
        }
        Ok(())
    }

    #[tokio::test]
    async fn test_fetching_groups_works() -> Result<()> {
        let mut tc = connector_setup().context("running tableau connector setup")?;
        let groups = tc.client.get_groups().await?;
        for g in groups {
            println!("{:#?}", g);
        }
        Ok(())
    }

    fn connector_setup() -> Result<TableauConnector> {
        let j = jetty::Jetty::new().context("creating Jetty")?;
        let creds = jetty::fetch_credentials().context("fetching credentials from file")?;
        let config = &j.config.connectors[0];
        let tc = TableauConnector::new(config, &creds["tableau"], None)
            .context("reading tableau credentials")?;
        Ok(*tc)
    }
}
