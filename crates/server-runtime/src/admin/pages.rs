use crate::hosting::replies::CommandPayloadView as View;
use minimium_commands::operation::{CommandValue as V, FiniteValue};

fn record<const N: usize>(fields: [(&str, V); N]) -> V {
    V::Record(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
}

pub(super) fn project(view: &View<'_>) -> Option<V> {
    let (rows, cursor, truncated) = match view {
        View::Players {
            rows,
            next_cursor,
            truncated,
        } => (
            rows.iter()
                .map(|r| {
                    record([
                        ("id", V::U64(r.id().get())),
                        ("name", V::Text(r.name().into())),
                        ("world", V::U128(r.world().get())),
                        (
                            "position",
                            V::Page {
                                rows: r
                                    .position()
                                    .iter()
                                    .map(|x| {
                                        FiniteValue::new(f64::from(*x)).map_or(V::Unit, V::Finite)
                                    })
                                    .collect(),
                                next_cursor: None,
                            },
                        ),
                    ])
                })
                .collect(),
            *next_cursor,
            *truncated,
        ),
        View::WorldList {
            rows,
            next_cursor,
            truncated,
        } => (
            rows.iter()
                .map(|r| {
                    record([
                        ("id", V::U128(r.id().get())),
                        ("players", V::U64(r.players() as u64)),
                        (
                            "topology",
                            V::Enum(
                                match r.topology() {
                                    minimium_world::world::WorldTopology::Planet => "planet",
                                    minimium_world::world::WorldTopology::Dungeon => "dungeon",
                                }
                                .into(),
                            ),
                        ),
                        ("generation_revision", V::U64(r.generation().get().into())),
                    ])
                })
                .collect(),
            *next_cursor,
            *truncated,
        ),
        View::EntityList {
            rows,
            next_cursor,
            truncated,
        } => (
            rows.iter()
                .map(|r| match r.kind() {
                    crate::EntityKindView::Entity(id) => record([
                        ("world", V::U128(r.world().get())),
                        ("kind", V::Enum("entity".into())),
                        ("id", V::U64(id)),
                    ]),
                    crate::EntityKindView::Player(id, name) => record([
                        ("world", V::U128(r.world().get())),
                        ("kind", V::Enum("player".into())),
                        ("id", V::U64(id.get())),
                        ("name", V::Text(name.into())),
                    ]),
                })
                .collect(),
            *next_cursor,
            *truncated,
        ),
        View::EntityInspect {
            rows,
            world,
            entity,
            next_cursor,
            truncated,
        } => {
            return Some(record([
                ("world", V::U128(world.get())),
                ("entity", V::U64(entity.get())),
                ("truncated", V::Bool(*truncated)),
                (
                    "components",
                    V::Page {
                        rows: rows
                            .iter()
                            .map(|r| {
                                record([
                                    ("kind", V::U64(r.kind().get().into())),
                                    ("schema", V::U64(r.schema().get().into())),
                                    ("bytes", V::U64(r.bytes() as u64)),
                                ])
                            })
                            .collect(),
                        next_cursor: next_cursor.map(str::to_owned),
                    },
                ),
            ]))
        }
        _ => return None,
    };
    Some(record([
        (
            "page",
            V::Page {
                rows,
                next_cursor: cursor.map(str::to_owned),
            },
        ),
        ("truncated", V::Bool(truncated)),
    ]))
}
