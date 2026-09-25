use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde_json::Value;

use crate::{config, model::RouterRecord};

#[derive(Clone)]
pub struct RouterOs {
    client: Client,
    url: String,
    username: String,
    password: String,
}

impl RouterOs {
    pub fn new(cfg: &config::Config) -> Result<Self> {
        let mut builder = Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(cfg.skip_tls_verify);
        if let Some(bytes) = config::ca_bytes(&cfg.ca_cert)? {
            builder = builder.add_root_certificate(
                reqwest::Certificate::from_pem(&bytes).context("parse CA certificate")?,
            );
        }
        Ok(Self {
            client: builder.build()?,
            url: cfg.base_url.clone(),
            username: cfg.username.clone(),
            password: cfg.password.clone(),
        })
    }
    pub async fn list(&self) -> Result<Vec<RouterRecord>> {
        self.client
            .get(&self.url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
            .context("decode RouterOS records")
    }
    pub async fn create(&self, record: &RouterRecord) -> Result<()> {
        let response = self
            .client
            .put(&self.url)
            .basic_auth(&self.username, Some(&self.password))
            .json(record)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!(
                "create RouterOS record: HTTP {status}: {}",
                response.text().await?
            );
        }
        Ok(())
    }
    pub async fn update(&self, id: &str, record: &RouterRecord) -> Result<()> {
        let mut payload = serde_json::to_value(record)?;
        let object = payload.as_object_mut().context("RouterOS update payload")?;
        object
            .entry("regexp")
            .or_insert_with(|| Value::String(String::new()));
        object
            .entry("match-subdomain")
            .or_insert_with(|| Value::String("false".into()));
        object
            .entry("address-list")
            .or_insert_with(|| Value::String(String::new()));
        object
            .entry("disabled")
            .or_insert_with(|| Value::String("false".into()));
        let response = self
            .client
            .patch(format!("{}/{id}", self.url))
            .basic_auth(&self.username, Some(&self.password))
            .json(&payload)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            anyhow::bail!(
                "update RouterOS record: HTTP {status}: {}",
                response.text().await?
            );
        }
        Ok(())
    }
    pub async fn delete(&self, id: &str) -> Result<()> {
        let response = self
            .client
            .delete(format!("{}/{id}", self.url))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        response
            .error_for_status()
            .map(|_| ())
            .context("delete RouterOS record")
    }
}
