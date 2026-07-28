use std::{fs, path::PathBuf};

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::{RngCore, rngs::OsRng};

#[derive(Clone, Debug)]
pub(crate) struct GithubAppConfig {
    pub(crate) app_id: u64,
    pub(crate) private_key_pem: String,
}

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) bind: String,
    pub(crate) database_url: String,
    pub(crate) admin_email: Option<String>,
    pub(crate) admin_password: Option<String>,
    pub(crate) cookie_secure: bool,
    pub(crate) github_app: Option<GithubAppConfig>,
    pub(crate) github_api_url: String,
    pub(crate) credential_key: Option<[u8; 32]>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self> {
        let admin_email = std::env::var("SUSUMU_ADMIN_EMAIL").ok();
        let admin_password = std::env::var("SUSUMU_ADMIN_PASSWORD").ok();
        if admin_email.is_some() != admin_password.is_some() {
            bail!("SUSUMU_ADMIN_EMAIL and SUSUMU_ADMIN_PASSWORD must be set together");
        }
        if let Some(password) = &admin_password
            && password.len() < 12
        {
            bail!("SUSUMU_ADMIN_PASSWORD must contain at least 12 characters");
        }

        let github_app = github_app_from_env()?;
        let credential_key = non_empty_env("SUSUMU_CREDENTIAL_KEY")
            .map(|value| {
                STANDARD
                    .decode(value)
                    .context("SUSUMU_CREDENTIAL_KEY must be base64")
                    .and_then(|bytes| {
                        bytes.try_into().map_err(|bytes: Vec<u8>| {
                            anyhow::anyhow!(
                                "SUSUMU_CREDENTIAL_KEY must decode to 32 bytes, got {}",
                                bytes.len()
                            )
                        })
                    })
            })
            .transpose()?;

        Ok(Self {
            bind: std::env::var("SUSUMU_BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned()),
            database_url: std::env::var("SUSUMU_DATABASE_URL")
                .context("SUSUMU_DATABASE_URL is required")?,
            admin_email: admin_email.map(|value| value.trim().to_lowercase()),
            admin_password,
            cookie_secure: std::env::var("SUSUMU_COOKIE_SECURE")
                .map_or(true, |value| value != "0" && value != "false"),
            github_app,
            github_api_url: std::env::var("SUSUMU_GITHUB_API_URL")
                .unwrap_or_else(|_| "https://api.github.com".to_owned()),
            credential_key,
        })
    }

    pub(crate) fn encrypt_private_key(&self, private_key_pem: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let key = self.credential_key.ok_or_else(|| {
            anyhow::anyhow!("SUSUMU_CREDENTIAL_KEY is required for GitHub App setup")
        })?;
        let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256 key has fixed length");
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), private_key_pem.as_bytes())
            .map_err(|_| anyhow::anyhow!("could not encrypt GitHub App private key"))?;
        Ok((ciphertext, nonce.to_vec()))
    }

    pub(crate) fn decrypt_private_key(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String> {
        let key = self.credential_key.ok_or_else(|| {
            anyhow::anyhow!("SUSUMU_CREDENTIAL_KEY is required to load GitHub App setup")
        })?;
        if nonce.len() != 12 {
            bail!("stored GitHub App private key nonce is invalid");
        }
        let cipher = Aes256Gcm::new_from_slice(&key).expect("AES-256 key has fixed length");
        let plaintext = cipher
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| anyhow::anyhow!("could not decrypt GitHub App private key"))?;
        String::from_utf8(plaintext).context("GitHub App private key is not UTF-8")
    }
}

fn github_app_from_env() -> Result<Option<GithubAppConfig>> {
    let app_id = non_empty_env("SUSUMU_GITHUB_APP_ID");
    let private_key_file = non_empty_env("SUSUMU_GITHUB_APP_PRIVATE_KEY_FILE");
    if app_id.is_none() != private_key_file.is_none() {
        bail!("SUSUMU_GITHUB_APP_ID and SUSUMU_GITHUB_APP_PRIVATE_KEY_FILE must be set together");
    }
    let (Some(app_id), Some(private_key_file)) = (app_id, private_key_file) else {
        return Ok(None);
    };
    let app_id = app_id
        .parse::<u64>()
        .context("SUSUMU_GITHUB_APP_ID must be a positive integer")?;
    if app_id == 0 {
        bail!("SUSUMU_GITHUB_APP_ID must be a positive integer");
    }
    let private_key_file = PathBuf::from(private_key_file);
    let private_key_pem = fs::read_to_string(&private_key_file).with_context(|| {
        format!(
            "could not read SUSUMU_GITHUB_APP_PRIVATE_KEY_FILE {}",
            private_key_file.display()
        )
    })?;
    if !private_key_pem.contains("-----BEGIN") || !private_key_pem.contains("PRIVATE KEY-----") {
        bail!("SUSUMU_GITHUB_APP_PRIVATE_KEY_FILE must contain a PEM private key");
    }
    Ok(Some(GithubAppConfig {
        app_id,
        private_key_pem,
    }))
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .and_then(|value| non_empty_value(&value))
}

fn non_empty_value(value: &str) -> Option<String> {
    let value = value.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::{Config, GithubAppConfig, non_empty_value};

    #[test]
    fn cookie_secure_defaults_to_true_when_configured() {
        let config = Config {
            bind: "127.0.0.1:8080".to_owned(),
            database_url: "postgres://localhost/susumu".to_owned(),
            admin_email: None,
            admin_password: None,
            cookie_secure: true,
            github_app: None,
            github_api_url: "https://api.github.com".to_owned(),
            credential_key: None,
        };
        assert!(config.cookie_secure);
    }

    #[test]
    fn github_app_configuration_keeps_key_material_in_server_config() {
        let config = GithubAppConfig {
            app_id: 42,
            private_key_pem: "-----BEGIN PRIVATE KEY-----\nkey\n-----END PRIVATE KEY-----"
                .to_owned(),
        };
        assert_eq!(config.app_id, 42);
        assert!(config.private_key_pem.contains("PRIVATE KEY"));
    }

    #[test]
    fn blank_optional_values_are_not_treated_as_github_configuration() {
        assert_eq!(non_empty_value("  "), None);
        assert_eq!(non_empty_value(" value "), Some("value".to_owned()));
    }

    #[test]
    fn encrypted_private_keys_round_trip_without_plaintext_storage() {
        let config = Config {
            bind: "127.0.0.1:8080".to_owned(),
            database_url: "postgres://localhost/susumu".to_owned(),
            admin_email: None,
            admin_password: None,
            cookie_secure: true,
            github_app: None,
            github_api_url: "https://api.github.com".to_owned(),
            credential_key: Some([7; 32]),
        };
        let pem = "-----BEGIN PRIVATE KEY-----\nsecret\n-----END PRIVATE KEY-----";
        let (ciphertext, nonce) = config.encrypt_private_key(pem).expect("encrypt key");
        assert_ne!(ciphertext, pem.as_bytes());
        assert_eq!(
            config.decrypt_private_key(&ciphertext, &nonce).unwrap(),
            pem
        );
    }
}
