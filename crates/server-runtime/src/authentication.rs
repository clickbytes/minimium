//! Nonblocking authentication port and bounded provider contracts.
use minimium_core::identity::Principal;
use std::sync::Arc;

pub const MAX_PROVIDER_ID_BYTES: usize = 32;
pub const MAX_CREDENTIAL_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AuthenticationAttemptId(u64);

impl AuthenticationAttemptId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }
    pub const fn get(self) -> u64 {
        self.0
    }
}

pub struct AuthenticationRequest {
    pub attempt_id: AuthenticationAttemptId,
    pub provider_id: String,
    pub credentials: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthenticationFailure {
    code: &'static str,
}

impl AuthenticationFailure {
    pub const fn new(code: &'static str) -> Self {
        Self { code }
    }
    pub const fn code(&self) -> &'static str {
        self.code
    }
}

pub enum AuthenticationStatus {
    Pending,
    Authenticated(Principal),
    Rejected(AuthenticationFailure),
}

/// Implementations must return promptly from `begin`, `poll` and `cancel`.
pub trait AuthenticationProvider: Send + Sync {
    fn begin(&self, request: AuthenticationRequest) -> Result<(), AuthenticationFailure>;
    fn poll(&self, attempt: AuthenticationAttemptId) -> AuthenticationStatus;
    fn cancel(&self, attempt: AuthenticationAttemptId);
}

pub type SharedAuthenticationProvider = Arc<dyn AuthenticationProvider>;
