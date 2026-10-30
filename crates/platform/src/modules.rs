//! Static, composable registration of engine capabilities.
//! Registration is explicit at startup; neither the wire protocol nor a Rust ABI
//! is extended implicitly by installing a module.
use minimium_core::entity::ComponentDescriptor;
use std::collections::BTreeMap;

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
    RenderPreparation,
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
}

/// Hosts provide their own handler types. A command handler need not know the
/// resources or privileges belonging to a different host/runtime.
pub struct CommandDefinition<H> {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub scope: CommandScope,
    pub handler: H,
}

pub struct SystemDefinition<S> {
    pub id: &'static str,
    pub stage: Stage,
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
    commands: Vec<RegisteredCommand<H>>,
    systems: Vec<RegisteredSystem<S>>,
    components: Vec<RegisteredComponent>,
}

impl<H, S> Default for ModuleRegistry<H, S> {
    fn default() -> Self {
        Self {
            modules: BTreeMap::new(),
            commands: Vec::new(),
            systems: Vec::new(),
            components: Vec::new(),
        }
    }
}

pub trait EngineModule<H, S> {
    fn descriptor(&self) -> ModuleDescriptor;
    fn register(&self, registrar: &mut ModuleRegistrar<'_, H, S>) -> Result<(), ModuleError>;
}

pub struct ModuleRegistrar<'a, H, S> {
    module: &'static str,
    existing_commands: &'a [RegisteredCommand<H>],
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
                });
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
    /// Installation is transactional: errors leave the live registry unchanged.
    /// Dependencies must already have been installed, establishing a DAG order.
    pub fn install<M: EngineModule<H, S>>(&mut self, module: &M) -> Result<(), ModuleError> {
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
        let mut staging = ModuleRegistrar {
            module: descriptor.id,
            existing_commands: &self.commands,
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
        self.systems.sort_by_key(|entry| entry.definition.stage);
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

    /// Longest path wins: `world inspect` takes priority over `world`.
    /// Arguments are never evaluated as code and remain caller-owned slices.
    pub fn resolve<'a>(&self, line: &'a str) -> Option<(&RegisteredCommand<H>, Vec<&'a str>)> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        self.commands
            .iter()
            .filter_map(|entry| {
                std::iter::once(entry.definition.name)
                    .chain(entry.definition.aliases.iter().copied())
                    .filter_map(|name| {
                        let command_parts: Vec<&str> = name.split_whitespace().collect();
                        (parts.starts_with(&command_parts)).then_some((entry, command_parts.len()))
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
