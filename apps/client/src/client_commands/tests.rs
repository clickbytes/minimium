use super::{command_specs, parse_command, ClientCommand, ParseError};

#[test]
fn portal_is_available_to_the_text_console_and_completion() {
    assert_eq!(
        parse_command("portal"),
        Ok(Some(ClientCommand::InteractPortal))
    );
    assert!(command_specs().iter().any(|spec| spec.name == "portal"));
}

#[test]
fn aliases_resolve_through_registered_parsers_and_admin_mutation_is_not_a_client_command() {
    assert_eq!(
        parse_command("chat hello"),
        Ok(Some(ClientCommand::Chat("hello".into())))
    );
    assert_eq!(
        parse_command("timescale 1"),
        Ok(Some(ClientCommand::TimeScale("1".into())))
    );
    assert_eq!(
        parse_command("time mode real_time"),
        Err(ParseError::UnknownCommand)
    );
}
