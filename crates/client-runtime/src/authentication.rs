//! Application-supplied bounded credentials for one connection attempt.
use std::{error::Error, fmt};

#[derive(Clone)]
pub struct ClientAuthentication {
    provider_id: String,
    credentials: Vec<u8>,
}

impl ClientAuthentication {
    pub fn new(
        provider_id: impl Into<String>,
        credentials: Vec<u8>,
    ) -> Result<Self, ClientAuthenticationError> {
        let provider_id = provider_id.into();
        if provider_id.is_empty()
            || provider_id.len() > minimium_protocol::MAX_AUTH_PROVIDER_BYTES
            || !provider_id.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            })
            || credentials.len() > minimium_protocol::MAX_AUTH_CREDENTIAL_BYTES
        {
            return Err(ClientAuthenticationError);
        }
        Ok(Self {
            provider_id,
            credentials,
        })
    }

    pub(crate) fn provider_id(&self) -> &str {
        &self.provider_id
    }
    pub(crate) fn credentials(&self) -> &[u8] {
        &self.credentials
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClientAuthenticationError;

impl fmt::Display for ClientAuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid authentication provider or credential size")
    }
}

impl Error for ClientAuthenticationError {}
