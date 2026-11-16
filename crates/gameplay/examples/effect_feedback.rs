//! Compile-only neutral module showing ordinary and Startup effect feedback.
#[path = "support/effect_feedback_module.rs"]
mod effect_feedback_module;

use effect_feedback_module::EffectFeedbackModule;
use minimium_gameplay::{GameplaySystem, ModuleCommandHandler, ModuleError, ModuleRegistry};

fn main() -> Result<(), ModuleError> {
    let mut registry = ModuleRegistry::<ModuleCommandHandler, GameplaySystem>::default();
    registry.install(&EffectFeedbackModule)
}
