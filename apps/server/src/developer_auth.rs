//! Explicit, insecure profile-declaration provider for local development.
use minimium_core::identity::{PermissionSet, PlayerProfileId, Principal, PrincipalId};
use minimium_server_runtime::authentication::{
    AuthenticationAttemptId, AuthenticationFailure, AuthenticationProvider, AuthenticationRequest,
    AuthenticationStatus,
};
use std::{collections::HashMap, sync::Mutex};

#[derive(Default)]
pub(super) struct DeveloperAuthenticationProvider {
    dev_mode: bool,
    ready: Mutex<HashMap<AuthenticationAttemptId, Principal>>,
}

impl DeveloperAuthenticationProvider {
    pub(super) fn new(dev_mode: bool) -> Self {
        Self {
            dev_mode,
            ready: Mutex::new(HashMap::new()),
        }
    }
}

impl AuthenticationProvider for DeveloperAuthenticationProvider {
    fn begin(&self, request: AuthenticationRequest) -> Result<(), AuthenticationFailure> {
        if request.provider_id != "dev.profile" || request.credentials.len() != 16 {
            return Err(AuthenticationFailure::new("auth.dev_credential_invalid"));
        }
        let bytes: [u8; 16] = request
            .credentials
            .try_into()
            .map_err(|_| AuthenticationFailure::new("auth.dev_credential_invalid"))?;
        let profile = PlayerProfileId::from_bytes(bytes);
        let principal = Principal::new(
            PrincipalId::from_bytes(bytes),
            profile,
            if self.dev_mode {
                PermissionSet::GAMEPLAY
                    .with(minimium_core::identity::ServerPermission::DeveloperFly)
                    .with(minimium_core::identity::ServerPermission::DeveloperMovementPolicy)
                    .with(minimium_core::identity::ServerPermission::DeveloperTeleport)
            } else {
                PermissionSet::GAMEPLAY
            },
        );
        self.ready
            .lock()
            .unwrap()
            .insert(request.attempt_id, principal);
        Ok(())
    }

    fn poll(&self, attempt: AuthenticationAttemptId) -> AuthenticationStatus {
        self.ready
            .lock()
            .unwrap()
            .remove(&attempt)
            .map(AuthenticationStatus::Authenticated)
            .unwrap_or_else(|| {
                AuthenticationStatus::Rejected(AuthenticationFailure::new("auth.attempt_unknown"))
            })
    }

    fn cancel(&self, attempt: AuthenticationAttemptId) {
        self.ready.lock().unwrap().remove(&attempt);
    }
}
