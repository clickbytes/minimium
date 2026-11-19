use crate::modules::{ScheduleError, Stage};

/// Concrete owner operations are declared and dispatched only by this host crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BuiltinOperation {
    Bolids,
    Impacts,
    WorldInputs,
    Simulation,
    ClearEvents,
}

impl BuiltinOperation {
    const ALL: [Self; 5] = [
        Self::Bolids,
        Self::Impacts,
        Self::WorldInputs,
        Self::Simulation,
        Self::ClearEvents,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::Bolids => "engine bolids",
            Self::Impacts => "engine impacts",
            Self::WorldInputs => "engine world inputs",
            Self::Simulation => minimium_gameplay::SIMULATION_SCHEDULE_ANCHOR,
            Self::ClearEvents => "engine clear events",
        }
    }

    pub(super) fn from_id(id: &str) -> Result<Self, ScheduleError> {
        Self::ALL
            .into_iter()
            .find(|operation| operation.id() == id)
            .ok_or(ScheduleError::UnknownOperation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeScheduleStep {
    Operation(BuiltinOperation),
    Stage(Stage),
}

/// Static production composition preserves the fixed order and publication barriers.
pub(crate) const fn production_schedule() -> minimium_gameplay::ScheduleDefinition {
    use minimium_gameplay::{ScheduleNode as Node, ScheduleNodeKind as Kind};
    const OPS: &[Node] = &[
        Node {
            id: "engine bolids",
            kind: Kind::Operation,
            before: &[],
            after: &[],
        },
        Node {
            id: "engine impacts",
            kind: Kind::Operation,
            before: &[],
            after: &["engine bolids"],
        },
        Node {
            id: "engine world inputs",
            kind: Kind::Operation,
            before: &[],
            after: &["engine impacts"],
        },
        Node {
            id: minimium_gameplay::FIXED_UPDATE_SCHEDULE_ANCHOR,
            kind: Kind::StageBarrier(Stage::FixedUpdate),
            before: &[],
            after: &["engine world inputs"],
        },
        Node {
            id: minimium_gameplay::SIMULATION_SCHEDULE_ANCHOR,
            kind: Kind::Operation,
            before: &[],
            after: &[minimium_gameplay::FIXED_UPDATE_SCHEDULE_ANCHOR],
        },
        Node {
            id: minimium_gameplay::ENVIRONMENT_SCHEDULE_ANCHOR,
            kind: Kind::StageBarrier(Stage::Environment),
            before: &[],
            after: &[minimium_gameplay::SIMULATION_SCHEDULE_ANCHOR],
        },
        Node {
            id: minimium_gameplay::POST_SIMULATION_SCHEDULE_ANCHOR,
            kind: Kind::StageBarrier(Stage::PostSimulation),
            before: &[],
            after: &[minimium_gameplay::ENVIRONMENT_SCHEDULE_ANCHOR],
        },
        Node {
            id: "engine clear events",
            kind: Kind::Operation,
            before: &[],
            after: &[minimium_gameplay::POST_SIMULATION_SCHEDULE_ANCHOR],
        },
    ];
    const REQUIRED: &[Stage] = &[
        Stage::Startup,
        Stage::FixedUpdate,
        Stage::Environment,
        Stage::PostSimulation,
    ];
    minimium_gameplay::ScheduleDefinition::new(
        OPS,
        minimium_gameplay::STARTUP_SCHEDULE_ANCHOR,
        REQUIRED,
    )
}
