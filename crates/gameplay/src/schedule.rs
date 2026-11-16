//! Host-supplied schedule graphs and their immutable validated execution order.
use crate::{ModuleRegistry, RegisteredSystem, Stage};
use std::collections::BTreeSet;

/// Stable, neutral IDs reserved for the lifecycle and publication anchors.
pub const STARTUP_SCHEDULE_ANCHOR: &str = "schedule startup";
pub const FIXED_UPDATE_SCHEDULE_ANCHOR: &str = "engine fixed effects";
pub const ENVIRONMENT_SCHEDULE_ANCHOR: &str = "engine environment effects";
pub const POST_SIMULATION_SCHEDULE_ANCHOR: &str = "engine post simulation effects";
/// Stable default-host anchor for work that follows the simulation operation.
pub const SIMULATION_SCHEDULE_ANCHOR: &str = "engine simulation";

pub const fn stage_anchor_id(stage: Stage) -> &'static str {
    match stage {
        Stage::Startup => STARTUP_SCHEDULE_ANCHOR,
        Stage::FixedUpdate => FIXED_UPDATE_SCHEDULE_ANCHOR,
        Stage::Environment => ENVIRONMENT_SCHEDULE_ANCHOR,
        Stage::PostSimulation => POST_SIMULATION_SCHEDULE_ANCHOR,
    }
}

/// A host-owned operation or a neutral effect-publication barrier in the host graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleNodeKind {
    Operation,
    StageBarrier(Stage),
}

/// One node in a host-supplied schedule. Its array position fixes its phase.
/// Registered systems share their stage barrier's phase; dependencies can order
/// systems within that batch but cannot move work backward across host phases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduleNode {
    pub id: &'static str,
    pub kind: ScheduleNodeKind,
    pub before: &'static [&'static str],
    pub after: &'static [&'static str],
}

/// Complete bootstrap definition for a host schedule and its required stage barriers.
pub struct ScheduleDefinition {
    pub nodes: &'static [ScheduleNode],
    /// Startup is a separate per-world lifecycle anchor and is never a tick node.
    pub startup_anchor: &'static str,
    pub required_stages: &'static [Stage],
}

impl ScheduleDefinition {
    pub const fn new(
        nodes: &'static [ScheduleNode],
        startup_anchor: &'static str,
        required_stages: &'static [Stage],
    ) -> Self {
        Self {
            nodes,
            startup_anchor,
            required_stages,
        }
    }
}

/// Typed graph validation failures. Invalid definitions never yield a partial plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleError {
    NodeCountOverflow,
    InvalidNodeId,
    DuplicateNodeId,
    ReservedAnchorCollision,
    DuplicateStageBarrier,
    MissingStageBarrier,
    StartupInTick,
    UnknownOperation,
    UnknownDependency,
    DuplicateDependency,
    InvalidPhaseOrder,
    DependencyCycle,
}

impl std::fmt::Display for ScheduleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "schedule validation failed: {self:?}")
    }
}

impl std::error::Error for ScheduleError {}

/// A validated, immutable sequence of owner operations and publication barriers.
/// Registered systems are retained in stable topological order for their stage batch.
pub struct SchedulePlan<'a, S> {
    steps: Vec<ScheduleStep>,
    systems: Vec<&'a RegisteredSystem<S>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleStep {
    Operation(&'static str),
    StageBarrier(Stage),
}

impl<S> SchedulePlan<'_, S> {
    pub fn steps(&self) -> &[ScheduleStep] {
        &self.steps
    }

    pub fn systems(&self) -> &[&RegisteredSystem<S>] {
        &self.systems
    }
}

impl<H, S> ModuleRegistry<H, S> {
    /// Validates this registry against the complete schedule supplied by its host.
    pub fn ordered_schedule(
        &self,
        definition: &ScheduleDefinition,
    ) -> Result<SchedulePlan<'_, S>, ScheduleError> {
        if definition.startup_anchor != STARTUP_SCHEDULE_ANCHOR {
            return Err(ScheduleError::InvalidNodeId);
        }
        let node_count = definition
            .nodes
            .len()
            .checked_add(1)
            .ok_or(ScheduleError::NodeCountOverflow)?;
        let count = node_count
            .checked_add(self.systems.len())
            .ok_or(ScheduleError::NodeCountOverflow)?;
        let mut nodes = Vec::with_capacity(node_count);
        nodes.push(ScheduleNode {
            id: definition.startup_anchor,
            kind: ScheduleNodeKind::StageBarrier(Stage::Startup),
            before: &[],
            after: &[],
        });
        nodes.extend_from_slice(definition.nodes);
        let mut stage_indices = [None; 4];
        let mut ids = BTreeSet::new();
        let mut last_recurring_stage = None;
        for (index, node) in nodes.iter().enumerate() {
            if !crate::valid_path(node.id) {
                return Err(ScheduleError::InvalidNodeId);
            }
            if matches!(node.kind, ScheduleNodeKind::StageBarrier(Stage::Startup)) && index != 0 {
                return Err(ScheduleError::StartupInTick);
            }
            if matches!(node.kind, ScheduleNodeKind::Operation) && is_reserved_anchor(node.id) {
                return Err(ScheduleError::ReservedAnchorCollision);
            }
            if !ids.insert(node.id) {
                return Err(ScheduleError::DuplicateNodeId);
            }
            match node.kind {
                ScheduleNodeKind::Operation => {}
                ScheduleNodeKind::StageBarrier(stage) => {
                    let slot = stage_index(stage);
                    if stage_indices[slot].is_some() {
                        return Err(ScheduleError::DuplicateStageBarrier);
                    }
                    if stage != Stage::Startup {
                        if last_recurring_stage.is_some_and(|previous| previous >= stage) {
                            return Err(ScheduleError::InvalidPhaseOrder);
                        }
                        last_recurring_stage = Some(stage);
                    }
                    if node.id != stage_anchor_id(stage) {
                        return Err(ScheduleError::InvalidNodeId);
                    }
                    stage_indices[slot] = Some(index);
                }
            }
        }
        let mut required = BTreeSet::new();
        for stage in definition.required_stages {
            if !required.insert(*stage) {
                return Err(ScheduleError::DuplicateStageBarrier);
            }
            if stage_indices[stage_index(*stage)].is_none() {
                return Err(ScheduleError::MissingStageBarrier);
            }
        }
        if !required.contains(&Stage::Startup) {
            return Err(ScheduleError::MissingStageBarrier);
        }
        if self
            .systems
            .iter()
            .any(|system| stage_indices[stage_index(system.definition.stage)].is_none())
        {
            return Err(ScheduleError::MissingStageBarrier);
        }

        let system_count = self.systems.len();
        debug_assert_eq!(count, nodes.len() + system_count);
        let phase = |vertex: usize| {
            if vertex < nodes.len() {
                vertex
            } else {
                stage_indices[stage_index(self.systems[vertex - nodes.len()].definition.stage)]
                    .expect("required system stage was validated")
            }
        };
        let id = |vertex: usize| {
            if vertex < nodes.len() {
                nodes[vertex].id
            } else {
                self.systems[vertex - nodes.len()].definition.id
            }
        };
        let mut outgoing = vec![BTreeSet::new(); count];
        let mut indegree = vec![0usize; count];
        let add_edge = |from: usize,
                        to: usize,
                        outgoing: &mut Vec<BTreeSet<usize>>,
                        indegree: &mut Vec<usize>| {
            if outgoing[from].insert(to) {
                indegree[to] += 1;
            }
        };

        let mut declared_edges = BTreeSet::new();
        for (index, node) in nodes.iter().enumerate() {
            add_declared_edges(
                index,
                node.before,
                node.after,
                &nodes,
                &self.systems,
                nodes.len(),
                &phase,
                &mut declared_edges,
                &mut outgoing,
                &mut indegree,
            )?;
        }
        for (index, system) in self.systems.iter().enumerate() {
            let vertex = nodes.len() + index;
            if is_reserved_anchor(system.definition.id) {
                return Err(ScheduleError::ReservedAnchorCollision);
            }
            if ids.contains(system.definition.id) {
                return Err(ScheduleError::DuplicateNodeId);
            }
            if self.systems[..index]
                .iter()
                .any(|other| other.definition.id == system.definition.id)
            {
                return Err(ScheduleError::DuplicateNodeId);
            }
            let barrier = phase(vertex);
            add_edge(vertex, barrier, &mut outgoing, &mut indegree);
            let definition = &system.definition;
            add_declared_edges(
                vertex,
                definition.before,
                definition.after,
                &nodes,
                &self.systems,
                nodes.len(),
                &phase,
                &mut declared_edges,
                &mut outgoing,
                &mut indegree,
            )?;
        }

        let mut ready: BTreeSet<_> = (0..count)
            .filter(|vertex| indegree[*vertex] == 0)
            .map(|vertex| (phase(vertex), id(vertex), vertex))
            .collect();
        let mut plan = SchedulePlan {
            steps: Vec::with_capacity(nodes.len()),
            systems: Vec::with_capacity(system_count),
        };
        let mut visited = 0;
        while let Some((_, _, vertex)) = ready.pop_first() {
            visited += 1;
            if vertex < nodes.len() {
                match nodes[vertex].kind {
                    ScheduleNodeKind::Operation => {
                        plan.steps.push(ScheduleStep::Operation(nodes[vertex].id));
                    }
                    ScheduleNodeKind::StageBarrier(Stage::Startup) => {}
                    ScheduleNodeKind::StageBarrier(stage) => {
                        plan.steps.push(ScheduleStep::StageBarrier(stage));
                    }
                }
            } else {
                plan.systems.push(&self.systems[vertex - nodes.len()]);
            }
            for target in &outgoing[vertex] {
                indegree[*target] -= 1;
                if indegree[*target] == 0 {
                    ready.insert((phase(*target), id(*target), *target));
                }
            }
        }
        if visited != count {
            return Err(ScheduleError::DependencyCycle);
        }
        Ok(plan)
    }
}

fn add_declared_edges<S>(
    vertex: usize,
    before: &[&'static str],
    after: &[&'static str],
    nodes: &[ScheduleNode],
    systems: &[crate::RegisteredSystem<S>],
    node_count: usize,
    phase: &impl Fn(usize) -> usize,
    declared_edges: &mut BTreeSet<(usize, usize)>,
    outgoing: &mut Vec<BTreeSet<usize>>,
    indegree: &mut Vec<usize>,
) -> Result<(), ScheduleError> {
    let mut declared = BTreeSet::new();
    for (target, reverse) in before
        .iter()
        .map(|id| (*id, false))
        .chain(after.iter().map(|id| (*id, true)))
    {
        if !declared.insert(target) {
            return Err(ScheduleError::DuplicateDependency);
        }
        let other = (0..node_count + systems.len())
            .find(|candidate| {
                if *candidate < node_count {
                    nodes[*candidate].id == target
                } else {
                    systems[*candidate - node_count].definition.id == target
                }
            })
            .ok_or(ScheduleError::UnknownDependency)?;
        let (from, to) = if reverse {
            (other, vertex)
        } else {
            (vertex, other)
        };
        if !declared_edges.insert((from, to)) {
            return Err(ScheduleError::DuplicateDependency);
        }
        if phase(from) > phase(to) {
            return Err(ScheduleError::InvalidPhaseOrder);
        }
        if phase(from) == phase(to)
            && from == other
            && nodes
                .get(other)
                .is_some_and(|node| matches!(node.kind, ScheduleNodeKind::StageBarrier(_)))
        {
            return Err(ScheduleError::InvalidPhaseOrder);
        }
        if outgoing[from].insert(to) {
            indegree[to] += 1;
        }
    }
    Ok(())
}

fn stage_index(stage: Stage) -> usize {
    match stage {
        Stage::Startup => 0,
        Stage::FixedUpdate => 1,
        Stage::Environment => 2,
        Stage::PostSimulation => 3,
    }
}

pub(super) fn is_reserved_anchor(id: &str) -> bool {
    [
        STARTUP_SCHEDULE_ANCHOR,
        FIXED_UPDATE_SCHEDULE_ANCHOR,
        ENVIRONMENT_SCHEDULE_ANCHOR,
        POST_SIMULATION_SCHEDULE_ANCHOR,
    ]
    .contains(&id)
}
