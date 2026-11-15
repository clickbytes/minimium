//! The executable tick skeleton and module dependencies share one validated graph.
use crate::{ModuleError, ModuleRegistry, RegisteredSystem, Stage};
use std::collections::BTreeSet;

/// Stable owner operations and effect-publication boundaries in a fixed tick.
/// A stage runs its ordered systems immediately before its publication boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuiltinOperation {
    Commands,
    Bolids,
    Impacts,
    Excavation,
    ShutdownGate,
    WorldInputs,
    FixedEffects,
    Simulation,
    EnvironmentEffects,
    PostSimulationEffects,
    ClearEvents,
}

impl BuiltinOperation {
    pub const ALL: [Self; 11] = [
        Self::Commands,
        Self::Bolids,
        Self::Impacts,
        Self::Excavation,
        Self::ShutdownGate,
        Self::WorldInputs,
        Self::FixedEffects,
        Self::Simulation,
        Self::EnvironmentEffects,
        Self::PostSimulationEffects,
        Self::ClearEvents,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Commands => "engine commands",
            Self::Bolids => "engine bolids",
            Self::Impacts => "engine impacts",
            Self::Excavation => "engine excavation",
            Self::ShutdownGate => "engine shutdown gate",
            Self::WorldInputs => "engine world inputs",
            Self::FixedEffects => "engine fixed effects",
            Self::Simulation => "engine simulation",
            Self::EnvironmentEffects => "engine environment effects",
            Self::PostSimulationEffects => "engine post simulation effects",
            Self::ClearEvents => "engine clear events",
        }
    }

    fn phase(self) -> u8 {
        match self {
            Self::Commands => 1,
            Self::Bolids => 2,
            Self::Impacts => 3,
            Self::Excavation => 4,
            Self::ShutdownGate => 5,
            Self::WorldInputs => 6,
            Self::FixedEffects => 8,
            Self::Simulation => 9,
            Self::EnvironmentEffects => 11,
            Self::PostSimulationEffects => 13,
            Self::ClearEvents => 14,
        }
    }
}

fn stage_phase(stage: Stage) -> u8 {
    match stage {
        Stage::Startup => 0,
        Stage::FixedUpdate => 7,
        Stage::Environment => 10,
        Stage::PostSimulation => 12,
    }
}

/// Immutable result of graph validation. Systems of a stage are contiguous;
/// the host executes them and commits their batch at that stage's boundary.
pub struct SchedulePlan<'a, S> {
    operations: Vec<BuiltinOperation>,
    systems: Vec<&'a RegisteredSystem<S>>,
}

impl<S> SchedulePlan<'_, S> {
    pub fn operations(&self) -> &[BuiltinOperation] {
        &self.operations
    }

    pub fn systems(&self) -> &[&RegisteredSystem<S>] {
        &self.systems
    }
}

impl<H, S> ModuleRegistry<H, S> {
    pub fn ordered_schedule(&self) -> Result<SchedulePlan<'_, S>, ModuleError> {
        let builtins = &BuiltinOperation::ALL;
        let count = builtins.len() + self.systems.len();
        let id = |index: usize| {
            if index < builtins.len() {
                builtins[index].id()
            } else {
                self.systems[index - builtins.len()].definition.id
            }
        };
        let phase = |index: usize| {
            if index < builtins.len() {
                builtins[index].phase()
            } else {
                stage_phase(self.systems[index - builtins.len()].definition.stage)
            }
        };
        let mut outgoing = vec![BTreeSet::new(); count];
        let mut indegree = vec![0usize; count];
        let mut edge = |from: usize, to: usize| {
            if outgoing[from].insert(to) {
                indegree[to] += 1;
            }
        };
        // Fixed owner order, including the shutdown and publication barriers.
        for index in 1..builtins.len() {
            edge(index - 1, index);
        }
        for (index, system) in self.systems.iter().enumerate() {
            if builtins.iter().any(|op| op.id() == system.definition.id) {
                return Err(ModuleError::DuplicateSystem);
            }
            let node = index + builtins.len();
            let boundary = builtins.partition_point(|op| op.phase() < phase(node));
            if boundary > 0 {
                edge(boundary - 1, node);
            }
            edge(node, boundary);
        }
        let mut declared_edges = BTreeSet::new();
        for (index, system) in self.systems.iter().enumerate() {
            let node = index + builtins.len();
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
                let other = (0..count)
                    .find(|index| id(*index) == target)
                    .ok_or(ModuleError::MissingDependency)?;
                let (from, to) = if reverse {
                    (other, node)
                } else {
                    (node, other)
                };
                if !declared_edges.insert((from, to)) {
                    return Err(ModuleError::InvalidDescriptor);
                }
                if phase(from) > phase(to) {
                    return Err(ModuleError::InvalidPhaseOrder);
                }
                edge(from, to);
            }
        }
        let mut ready: BTreeSet<_> = (0..count)
            .filter(|index| indegree[*index] == 0)
            .map(|index| (phase(index), id(index), index))
            .collect();
        let mut plan = SchedulePlan {
            operations: Vec::with_capacity(builtins.len()),
            systems: Vec::with_capacity(self.systems.len()),
        };
        let mut visited = 0;
        while let Some((_, _, index)) = ready.pop_first() {
            visited += 1;
            if index < builtins.len() {
                plan.operations.push(builtins[index]);
            } else {
                plan.systems.push(&self.systems[index - builtins.len()]);
            }
            for target in &outgoing[index] {
                indegree[*target] -= 1;
                if indegree[*target] == 0 {
                    ready.insert((phase(*target), id(*target), *target));
                }
            }
        }
        if visited != count {
            return Err(ModuleError::DependencyCycle);
        }
        Ok(plan)
    }
}
