use minimium_simulation::CombatSettings;

/// Sample-game combat values used by the authoritative host and client UI.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleCombatConfig {
    /// Parameters for the shared simulation hitscan rule.
    pub hitscan: CombatSettings,
    /// Minimum interval between accepted shots in milliseconds.
    pub fire_cooldown_ms: u64,
}

/// Existing prototype combat balance, now owned by sample-game content.
pub const DEFAULT_COMBAT_CONFIG: SampleCombatConfig = SampleCombatConfig {
    hitscan: CombatSettings {
        damage: 34,
        range: 60.0,
    },
    fire_cooldown_ms: 250,
};
