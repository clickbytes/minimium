//! Accept the canonical generation spelling at startup while retaining the
//! established `world.seed` resolver, file and CLI semantics.

use minimium_commands::{CommandError, CommandErrorCode};
use std::collections::BTreeMap;

pub(super) fn normalize(
    mut settings: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, CommandError> {
    let Some(canonical) = settings.remove("world.primary.generation.seed") else {
        return Ok(settings);
    };
    if let Some(legacy) = settings.get("world.seed") {
        let equal = legacy == &canonical
            || (super::values::parse_seed(legacy).ok()
                == super::values::parse_seed(&canonical).ok()
                && super::values::parse_seed(legacy).is_ok());
        if !equal {
            return Err(CommandError::new(
                CommandErrorCode::Conflict,
                Some("world.primary.generation.seed"),
                "generation seed aliases disagree",
                Some("use one seed value in configuration"),
            ));
        }
    }
    settings.insert("world.seed".into(), canonical);
    Ok(settings)
}
