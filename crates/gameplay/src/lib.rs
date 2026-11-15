//! Static, composable registration of engine capabilities.
//! Registration is explicit at startup; neither the wire protocol nor a Rust ABI
//! is extended implicitly by installing a module.
use minimium_commands::{lex_command_line, CommandArgumentSpec};
use minimium_world::entity::ComponentDescriptor;
use std::collections::{BTreeMap, BTreeSet};

mod effects;
mod system;
pub use effects::{
    ComponentValue, ComponentView, EffectBufferError, GameplayEvent, InitialComponentView,
    InitialEntityView, WorldEffect, WorldEffectBatch, WorldEffectBuffer, WorldLifecycle,
    MAX_WORLD_EFFECTS_PER_STAGE, MAX_WORLD_EFFECT_BYTES_PER_STAGE,
};
pub use system::{GameplaySystem, GameplaySystemContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleDescriptor {
    pub id: &'static str,
    pub version: u32,
    pub requires: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandScope {
    ClientLocal,
    ServerAdmin,
    PlayerAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Stage {
    Startup,
    FixedUpdate,
    Environment,
    PostSimulation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModuleError {
    DuplicateModule,
    MissingDependency,
    InvalidDescriptor,
    InvalidCommand,
    DuplicateCommand,
    DuplicateSystem,
    DuplicateComponent,
    DependencyCycle,
}

impl std::fmt::Display for ModuleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "gameplay module registration failed: {self:?}")
    }
}

impl std::error::Error for ModuleError {}

/// Hosts provide their own handler types. A command handler need not know the
/// resources or privileges belonging to a different host/runtime.
pub struct CommandDefinition<H> {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub arguments: &'static [CommandArgumentSpec],
    pub scope: CommandScope,
    pub handler: H,
}

pub struct SystemDefinition<S> {
    pub id: &'static str,
    pub stage: Stage,
    pub before: &'static [&'static str],
    pub after: &'static [&'static str],
    pub handler: S,
}

pub struct RegisteredCommand<H> {
    pub module: &'static str,
    pub definition: CommandDefinition<H>,
}

pub struct RegisteredSystem<S> {
    pub module: &'static str,
    pub definition: SystemDefinition<S>,
}

pub struct RegisteredComponent {
    pub module: &'static str,
    pub descriptor: ComponentDescriptor,
}

pub struct ModuleRegistry<H, S> {
    modules: BTreeMap<&'static str, ModuleDescriptor>,
    reserved_commands: BTreeSet<&'static str>,
    commands: Vec<RegisteredCommand<H>>,
    systems: Vec<RegisteredSystem<S>>,
    components: Vec<RegisteredComponent>,
}

impl<H, S> Default for ModuleRegistry<H, S> {
    fn default() -> Self {
        Self {
            modules: BTreeMap::new(),
            reserved_commands: BTreeSet::new(),
            commands: Vec::new(),
            systems: Vec::new(),
            components: Vec::new(),
        }
    }
}

pub trait EngineModule<H, S>: Send + Sync {
    fn descriptor(&self) -> ModuleDescriptor;
    fn register(&self, registrar: &mut ModuleRegistrar<'_, H, S>) -> Result<(), ModuleError>;
}

pub struct ModuleRegistrar<'a, H, S> {
    module: &'static str,
    existing_commands: &'a [RegisteredCommand<H>],
    reserved_commands: &'a BTreeSet<&'static str>,
    existing_systems: &'a [RegisteredSystem<S>],
    existing_components: &'a [RegisteredComponent],
    commands: Vec<RegisteredCommand<H>>,
    systems: Vec<RegisteredSystem<S>>,
    components: Vec<RegisteredComponent>,
}

fn valid_path(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name.split(' ').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
        })
}

impl<H, S> ModuleRegistrar<'_, H, S> {
    pub fn command(&mut self, definition: CommandDefinition<H>) -> Result<(), ModuleError> {
        let names = std::iter::once(definition.name).chain(definition.aliases.iter().copied());
        let mut seen = std::collections::HashSet::new();
        for name in names {
            if !valid_path(name) {
                return Err(ModuleError::InvalidCommand);
            }
            let clash = self
                .existing_commands
                .iter()
                .chain(self.commands.iter())
                .any(|entry| {
                    std::iter::once(entry.definition.name)
                        .chain(entry.definition.aliases.iter().copied())
                        .any(|registered| registered == name)
                })
                || self.reserved_commands.contains(name);
            if clash || !seen.insert(name) {
                return Err(ModuleError::DuplicateCommand);
            }
        }
        self.commands.push(RegisteredCommand {
            module: self.module,
            definition,
        });
        Ok(())
    }

    pub fn component(&mut self, descriptor: ComponentDescriptor) -> Result<(), ModuleError> {
        if self
            .existing_components
            .iter()
            .chain(self.components.iter())
            .any(|entry| {
                entry.descriptor.id == descriptor.id || entry.descriptor.name == descriptor.name
            })
        {
            return Err(ModuleError::DuplicateComponent);
        }
        self.components.push(RegisteredComponent {
            module: self.module,
            descriptor,
        });
        Ok(())
    }

    pub fn system(&mut self, definition: SystemDefinition<S>) -> Result<(), ModuleError> {
        if !valid_path(definition.id)
            || self
                .existing_systems
                .iter()
                .chain(self.systems.iter())
                .any(|entry| entry.definition.id == definition.id)
        {
            return Err(ModuleError::DuplicateSystem);
        }
        self.systems.push(RegisteredSystem {
            module: self.module,
            definition,
        });
        Ok(())
    }
}

impl<H, S> ModuleRegistry<H, S> {
    /// Reserves names owned by host built-ins before module installation begins.
    pub fn reserve_command_names(
        &mut self,
        names: &'static [&'static str],
    ) -> Result<(), ModuleError> {
        let mut staged = BTreeSet::new();
        for name in names {
            if !valid_path(name)
                || self.reserved_commands.contains(name)
                || self.commands.iter().any(|entry| {
                    std::iter::once(entry.definition.name)
                        .chain(entry.definition.aliases.iter().copied())
                        .any(|registered| registered == *name)
                })
                || !staged.insert(*name)
            {
                return Err(ModuleError::DuplicateCommand);
            }
        }
        self.reserved_commands.extend(staged);
        Ok(())
    }

    /// Installation is transactional: errors leave the live registry unchanged.
    /// Dependencies must already have been installed, establishing a DAG order.
    pub fn install<M: EngineModule<H, S> + ?Sized>(
        &mut self,
        module: &M,
    ) -> Result<(), ModuleError> {
        let descriptor = module.descriptor();
        if !valid_path(descriptor.id)
            || descriptor.version == 0
            || descriptor
                .requires
                .iter()
                .any(|dependency| !valid_path(dependency))
        {
            return Err(ModuleError::InvalidDescriptor);
        }
        if self.modules.contains_key(descriptor.id) {
            return Err(ModuleError::DuplicateModule);
        }
        if descriptor
            .requires
            .iter()
            .any(|dependency| !self.modules.contains_key(dependency))
        {
            return Err(ModuleError::MissingDependency);
        }
        if descriptor
            .requires
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .len()
            != descriptor.requires.len()
        {
            return Err(ModuleError::InvalidDescriptor);
        }
        let mut staging = ModuleRegistrar {
            module: descriptor.id,
            existing_commands: &self.commands,
            reserved_commands: &self.reserved_commands,
            existing_systems: &self.systems,
            existing_components: &self.components,
            components: Vec::new(),
            commands: Vec::new(),
            systems: Vec::new(),
        };
        module.register(&mut staging)?;
        let ModuleRegistrar {
            commands,
            systems,
            components,
            ..
        } = staging;
        self.modules.insert(descriptor.id, descriptor);
        self.commands.extend(commands);
        self.systems.extend(systems);
        self.components.extend(components);
        Ok(())
    }

    pub fn modules(&self) -> impl Iterator<Item = &ModuleDescriptor> {
        self.modules.values()
    }

    pub fn commands(&self) -> &[RegisteredCommand<H>] {
        &self.commands
    }

    pub fn components(&self) -> &[RegisteredComponent] {
        &self.components
    }

    pub fn systems(&self) -> &[RegisteredSystem<S>] {
        &self.systems
    }

    pub fn ordered_systems(&self) -> Result<Vec<&RegisteredSystem<S>>, ModuleError> {
        let mut outgoing = vec![Vec::new(); self.systems.len()];
        let mut indegree = vec![0usize; self.systems.len()];
        let mut declared_edges = BTreeSet::new();
        for (index, system) in self.systems.iter().enumerate() {
            let mut declared = BTreeSet::new();
            for (target, reverse) in system
                .definition
                .before
                .iter()
                .map(|id| (*id, false))
                .chain(system.definition.after.iter().map(|id| (*id, true)))
            {
                if !declared.insert(target) {
                    return Err(ModuleError::InvalidDescriptor);
                }
                let other = self
                    .systems
                    .iter()
                    .position(|candidate| candidate.definition.id == target)
                    .ok_or(ModuleError::MissingDependency)?;
                let (from, to) = if reverse {
                    (other, index)
                } else {
                    (index, other)
                };
                if !declared_edges.insert((from, to)) {
                    return Err(ModuleError::InvalidDescriptor);
                }
                if self.systems[from].definition.stage > self.systems[to].definition.stage {
                    return Err(ModuleError::InvalidDescriptor);
                }
                outgoing[from].push(to);
                indegree[to] += 1;
            }
        }
        let mut ready: BTreeSet<(Stage, &'static str, usize)> = self
            .systems
            .iter()
            .enumerate()
            .filter(|(index, _)| indegree[*index] == 0)
            .map(|(index, system)| (system.definition.stage, system.definition.id, index))
            .collect();
        let mut ordered = Vec::with_capacity(self.systems.len());
        while let Some((_, _, index)) = ready.pop_first() {
            ordered.push(&self.systems[index]);
            for target in &outgoing[index] {
                indegree[*target] -= 1;
                if indegree[*target] == 0 {
                    let system = &self.systems[*target];
                    ready.insert((system.definition.stage, system.definition.id, *target));
                }
            }
        }
        if ordered.len() != self.systems.len() {
            return Err(ModuleError::DependencyCycle);
        }
        Ok(ordered)
    }

    /// Longest path wins: `world inspect` takes priority over `world`.
    /// Arguments are never evaluated as code and remain caller-owned slices.
    pub fn resolve(&self, line: &str) -> Option<(&RegisteredCommand<H>, Vec<String>)> {
        let parts = lex_command_line(line).ok()?;
        let words = parts.iter().map(String::as_str).collect::<Vec<_>>();
        self.commands
            .iter()
            .filter_map(|entry| {
                std::iter::once(entry.definition.name)
                    .chain(entry.definition.aliases.iter().copied())
                    .filter_map(|name| {
                        let command_parts: Vec<&str> = name.split_whitespace().collect();
                        (words.starts_with(&command_parts)).then_some((entry, command_parts.len()))
                    })
                    .max_by_key(|(_, len)| *len)
            })
            .max_by_key(|(_, len)| *len)
            .map(|(entry, len)| (entry, parts[len..].to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SampleModule {
        id: &'static str,
        requires: &'static [&'static str],
        command: &'static str,
    }

    impl EngineModule<(), ()> for SampleModule {
        fn descriptor(&self) -> ModuleDescriptor {
            ModuleDescriptor {
                id: self.id,
                version: 1,
                requires: self.requires,
            }
        }

        fn register(&self, registrar: &mut ModuleRegistrar<'_, (), ()>) -> Result<(), ModuleError> {
            registrar.command(CommandDefinition {
                name: self.command,
                aliases: &[],
                usage: self.command,
                arguments: &[],
                scope: CommandScope::ServerAdmin,
                handler: (),
            })
        }
    }

    #[test]
    fn installation_is_transactional_and_rejects_dependency_and_name_collisions() {
        let mut registry: ModuleRegistry<(), ()> = ModuleRegistry::default();
        let first = SampleModule {
            id: "first",
            requires: &[],
            command: "world list",
        };
        let dependent = SampleModule {
            id: "second",
            requires: &["first"],
            command: "world inspect",
        };
        assert_eq!(
            registry.install(&dependent),
            Err(ModuleError::MissingDependency)
        );
        assert_eq!(registry.modules().count(), 0);
        registry.install(&first).unwrap();
        let collision = SampleModule {
            id: "third",
            requires: &[],
            command: "world list",
        };
        assert_eq!(
            registry.install(&collision),
            Err(ModuleError::DuplicateCommand)
        );
        assert_eq!(registry.modules().count(), 1);
        registry.install(&dependent).unwrap();
        assert_eq!(registry.resolve("world inspect 12").unwrap().1, vec!["12"]);
    }
}
