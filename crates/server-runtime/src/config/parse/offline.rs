use super::*;

pub(crate) fn offline_command(
    args: &[String],
    process_environment: &BTreeMap<String, String>,
) -> Result<Option<String>, CommandError> {
    if args.first().map(String::as_str) != Some("config") {
        return Ok(None);
    }
    let action = args.get(1).map(String::as_str).ok_or_else(|| {
        error(
            CommandErrorCode::InvalidArguments,
            "config",
            "expected validate or show",
            "config validate|show --config PATH",
        )
    })?;
    if !matches!(action, "validate" | "show") {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            "config",
            "expected validate or show",
            "config validate|show --config PATH",
        ));
    }
    let program = format!("minimium-server config {action} --config PATH");
    let parsed = match parse_cli(args.iter().skip(2).cloned(), &program, OFFLINE_OPTIONS)? {
        CliParseOutcome::Help => {
            return Ok(Some(format!(
                "{program}\n--sources and --format json are available with show"
            )))
        }
        CliParseOutcome::Parsed(parsed) => parsed,
    };
    if !parsed.positionals().is_empty() {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            parsed.positionals()[0].as_str(),
            "unexpected positional argument",
            &program,
        ));
    }
    if action == "validate" && (parsed.contains("--sources") || parsed.get("--format").is_some()) {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            "config validate",
            "format and sources are only available with show",
            &program,
        ));
    }
    let mut overrides = BTreeMap::new();
    for assignment in parsed.get_all("--set").flatten() {
        let (key, value) = assignment.split_once('=').ok_or_else(|| {
            error(
                CommandErrorCode::InvalidArguments,
                "--set",
                "expected KEY=VALUE",
                program.as_str(),
            )
        })?;
        if overrides.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(error(
                CommandErrorCode::DuplicateOption,
                key,
                "setting was supplied more than once",
                program.as_str(),
            ));
        }
    }
    let path = parsed.get("--config").map(Path::new);
    let resolved = resolve(path, overrides, process_environment)?;
    let _validated_runtime_config = config_from(resolved.clone())?;
    if action == "validate" {
        return Ok(Some(format!(
            "valid server configuration schema=1 settings={}",
            resolved.len()
        )));
    }
    let output_format = parsed.get("--format").unwrap_or("text");
    if !matches!(output_format, "text" | "json") {
        return Err(error(
            CommandErrorCode::InvalidValue,
            "--format",
            "format must be text or json",
            program.as_str(),
        ));
    }
    let include_sources = parsed.contains("--sources");
    if output_format == "json" {
        let settings = resolved
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    JsonSetting {
                        requested: value.requested.clone(),
                        effective: effective_value(key, &value.requested),
                        default: value.default.clone(),
                        source: value.source.as_str(),
                        reason: effective_reason(key, &value.requested),
                    },
                )
            })
            .collect();
        let output = serde_json::to_string(&JsonView {
            schema_version: 1,
            settings,
        })
        .map_err(|e| {
            error(
                CommandErrorCode::InvalidArguments,
                "config show",
                format!("JSON serialization failed: {e}"),
                "config show --format json",
            )
        })?;
        if output.len() > MAX_CONFIG_BYTES {
            return Err(error(
                CommandErrorCode::InvalidArguments,
                "config show",
                "resolved JSON output exceeds 64 KiB",
                "config show --format json",
            ));
        }
        Ok(Some(output))
    } else {
        Ok(Some(
            resolved
                .iter()
                .map(|(key, value)| {
                    let line = format!(
                        "{key}={} effective={}",
                        value.requested,
                        effective_value(key, &value.requested)
                    );
                    if include_sources {
                        format!(
                            "{line} default={} source={}",
                            value.default.as_deref().unwrap_or("n/a"),
                            value.source.as_str()
                        )
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }
}
