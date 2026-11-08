use super::{cli_error, ClientOptions, CommandError, CommandErrorCode};

pub(super) fn validate(options: &mut ClientOptions) -> Result<(), CommandError> {
    let local = options.connect.as_ref().is_none_or(|address| {
        address
            .parse::<std::net::SocketAddr>()
            .is_ok_and(|address| address.ip().is_loopback())
    });
    if options.dev_stdin && (!options.dev_mode || !local) {
        return Err(cli_error(
            CommandErrorCode::InvalidArguments,
            "--dev-stdin",
            "requires --dev with an owned server or an explicit loopback address",
        ));
    }
    if options.dev_stdin && options.connect.is_some() {
        // --dev acknowledges local diagnostics, never remote privilege.
        options.dev_mode = false;
    }
    if options.dev_stdin && options.console {
        return Err(cli_error(
            CommandErrorCode::InvalidArguments,
            "--dev-stdin",
            "desktop client is required",
        ));
    }
    Ok(())
}
