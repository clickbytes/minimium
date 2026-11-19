use super::*;
use minimium_gameplay::ModuleCommandReadView;

pub(crate) struct RuntimeCommandReadView<'a> {
    worlds: &'a WorldRegistry,
}

impl<'a> RuntimeCommandReadView<'a> {
    pub(crate) fn new(worlds: &'a WorldRegistry) -> Self {
        Self { worlds }
    }

    fn cursor_issuer(&self) -> CommandViewCursorIssuer {
        CommandViewCursorIssuer::new(self.worlds.read_identity.clone())
    }

    fn world_view(&self, id: WorldInstanceId) -> Option<CommandWorldView> {
        let instance = self.worlds.get(id)?;
        Some(CommandWorldView {
            id,
            players: instance.world.player_count(),
            topology: instance.manifest.topology(),
            generation: instance.manifest.generation().revision(),
            manifest: instance.manifest,
        })
    }

    fn validate_cursor(
        &self,
        request: &CommandViewRequest,
        query: CommandViewKind,
        scope: CommandViewScope,
        revisions: [u128; 3],
    ) -> Result<u128, CommandViewError> {
        let Some(cursor) = request.cursor() else {
            return Ok(0);
        };
        if !cursor.is_same_runtime(&self.worlds.read_identity)
            || cursor.query() != query
            || cursor.scope() != scope
            || cursor.revision() != revisions[0]
            || cursor.secondary_revision() != revisions[1]
            || cursor.tertiary_revision() != revisions[2]
        {
            return Err(CommandViewError::StaleCursor);
        }
        Ok(cursor.after_key())
    }
}

impl ModuleCommandReadView for RuntimeCommandReadView<'_> {
    fn parse_cursor(
        &self,
        token: &str,
    ) -> Result<minimium_gameplay::CommandViewCursor, CommandViewError> {
        self.cursor_issuer().parse(token)
    }
    fn world_count(&self) -> usize {
        self.worlds.iter().count()
    }

    fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError> {
        let revisions = [self.worlds.world_membership_revision, 0, 0];
        let after = self.validate_cursor(
            request,
            CommandViewKind::Worlds,
            CommandViewScope::None,
            revisions,
        )?;
        let mut candidates = self
            .worlds
            .iter()
            .filter(|(id, _)| id.get() as u128 > after)
            .filter_map(|(id, _)| self.world_view(id));
        let row_budget = if request.max_bytes() <= 32 * 1024 {
            192
        } else {
            std::mem::size_of::<CommandWorldView>()
        };
        let count = request.limit().min(request.max_bytes() / row_budget);
        if count == 0 {
            return Err(CommandViewError::RecordTooLarge);
        }
        let rows = candidates.by_ref().take(count).collect::<Vec<_>>();
        let next = if candidates.next().is_some() {
            rows.last().map(|last| {
                self.cursor_issuer().issue_versions(
                    CommandViewKind::Worlds,
                    CommandViewScope::None,
                    revisions[0],
                    revisions[1],
                    revisions[2],
                    last.id.get() as u128,
                )
            })
        } else {
            None
        };
        Ok(CommandViewPage::new(rows, next))
    }

    fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError> {
        self.world_view(id).ok_or(CommandViewError::UnknownWorld)
    }

    fn entities(
        &self,
        world: WorldInstanceId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError> {
        let instance = self
            .worlds
            .get(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        let revisions = [instance.entities.membership_revision() as u128, 0, 0];
        let scope = CommandViewScope::World(world);
        let after = self.validate_cursor(request, CommandViewKind::Entities, scope, revisions)?;
        let mut candidates = instance
            .entities
            .iter()
            .filter(|id| id.get() as u128 > after);
        let rows = candidates
            .by_ref()
            .take(request.limit())
            .collect::<Vec<_>>();
        let next = if candidates.next().is_some() {
            rows.last().map(|id| {
                self.cursor_issuer().issue_versions(
                    CommandViewKind::Entities,
                    scope,
                    revisions[0],
                    revisions[1],
                    revisions[2],
                    id.get() as u128,
                )
            })
        } else {
            None
        };
        Ok(CommandViewPage::new(rows, next))
    }

    fn players(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandPlayerView>, CommandViewError> {
        let revisions = [self.worlds.player_membership_revision, 0, 0];
        let after = self.validate_cursor(
            request,
            CommandViewKind::Players,
            CommandViewScope::None,
            revisions,
        )?;
        // Active players are globally capped by `config::MAX_PLAYERS` (512).
        let mut players = self
            .worlds
            .player_snapshots()
            .into_iter()
            .collect::<Vec<_>>();
        players.sort_by_key(|(_, player)| player.id.get());
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut last = after;
        let mut more = false;
        for (world, player) in players
            .into_iter()
            .filter(|(_, player)| player.id.get() as u128 > after)
        {
            let key = player.id.get() as u128;
            let row_bytes = std::mem::size_of::<CommandPlayerView>() + player.name.len();
            if rows.len() >= request.limit()
                || bytes.saturating_add(row_bytes) > request.max_bytes()
            {
                if rows.is_empty() {
                    return Err(CommandViewError::RecordTooLarge);
                }
                more = true;
                break;
            }
            bytes += row_bytes;
            rows.push(CommandPlayerView {
                id: player.id,
                name: player.name.to_string(),
                world,
                position: player.position,
            });
            last = key;
        }
        let next = more.then(|| {
            self.cursor_issuer().issue_versions(
                CommandViewKind::Players,
                CommandViewScope::None,
                revisions[0],
                revisions[1],
                revisions[2],
                last,
            )
        });
        Ok(CommandViewPage::new(rows, next))
    }

    fn entity_rows(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<minimium_gameplay::CommandEntityRow>, CommandViewError> {
        let revisions = [
            self.worlds.entity_membership_revision,
            self.worlds.player_membership_revision,
            self.worlds.world_membership_revision,
        ];
        let (after_world, after_key) = if let Some(cursor) = request.cursor() {
            if !cursor.is_same_runtime(&self.worlds.read_identity)
                || cursor.query() != CommandViewKind::EntityRows
                || cursor.revision() != revisions[0]
                || cursor.secondary_revision() != revisions[1]
                || cursor.tertiary_revision() != revisions[2]
            {
                return Err(CommandViewError::StaleCursor);
            }
            match cursor.scope() {
                CommandViewScope::World(world) => (world.get(), cursor.after_key()),
                _ => return Err(CommandViewError::StaleCursor),
            }
        } else {
            (0, 0)
        };
        let mut players = self
            .worlds
            .player_snapshots()
            .into_iter()
            .collect::<Vec<_>>();
        players.sort_by_key(|(world, player)| (world.get(), player.id.get()));
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut last_world = after_world;
        let mut last_key = after_key;
        let mut more = false;
        for (world, instance) in self
            .worlds
            .iter()
            .filter(|(world, _)| world.get() >= after_world)
        {
            let world_key = world.get();
            let starting_key = if world_key == after_world {
                after_key
            } else {
                0
            };
            for entity in instance
                .entities
                .iter()
                .filter(|id| id.get() as u128 > starting_key)
            {
                let row_key = entity.get() as u128;
                if rows.len() >= request.limit() {
                    more = true;
                    break;
                }
                let row_bytes = if request.max_bytes() <= 32 * 1024 {
                    224
                } else {
                    std::mem::size_of::<minimium_gameplay::CommandEntityRow>()
                };
                if bytes.saturating_add(row_bytes) > request.max_bytes() {
                    if rows.is_empty() {
                        return Err(CommandViewError::RecordTooLarge);
                    }
                    more = true;
                    break;
                }
                rows.push(minimium_gameplay::CommandEntityRow {
                    world,
                    kind: minimium_gameplay::CommandEntityKind::Entity(entity),
                });
                bytes += row_bytes;
                last_world = world_key;
                last_key = row_key;
            }
            if more {
                break;
            }
            for (_, player) in players.iter().filter(|(owner, player)| {
                owner.get() == world_key && ((1u128 << 64) | player.id.get() as u128) > starting_key
            }) {
                let row_key = (1u128 << 64) | player.id.get() as u128;
                if rows.len() >= request.limit() {
                    more = true;
                    break;
                }
                let row_bytes = if request.max_bytes() <= 32 * 1024 {
                    224 + player.name.len()
                } else {
                    std::mem::size_of::<minimium_gameplay::CommandEntityRow>() + player.name.len()
                };
                if bytes.saturating_add(row_bytes) > request.max_bytes() {
                    if rows.is_empty() {
                        return Err(CommandViewError::RecordTooLarge);
                    }
                    more = true;
                    break;
                }
                rows.push(minimium_gameplay::CommandEntityRow {
                    world,
                    kind: minimium_gameplay::CommandEntityKind::Player(
                        player.id,
                        player.name.to_string(),
                    ),
                });
                bytes += row_bytes;
                last_world = world_key;
                last_key = row_key;
            }
            if more {
                break;
            }
        }
        let next = more.then(|| {
            self.cursor_issuer().issue_versions(
                CommandViewKind::EntityRows,
                CommandViewScope::World(
                    WorldInstanceId::new(last_world).expect("cursor world ID is nonzero"),
                ),
                revisions[0],
                revisions[1],
                revisions[2],
                last_key,
            )
        });
        Ok(CommandViewPage::new(rows, next))
    }

    fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError> {
        let instance = self
            .worlds
            .get(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        if !instance.entities.contains(entity) {
            return Err(CommandViewError::UnknownEntity);
        }
        let revisions = [
            instance.entities.component_membership_revision() as u128,
            0,
            0,
        ];
        let scope = CommandViewScope::Entity { world, entity };
        let after = self.validate_cursor(request, CommandViewKind::Components, scope, revisions)?;
        let mut components = self
            .worlds
            .component_views(world)
            .ok_or(CommandViewError::UnknownWorld)?
            .filter(|(owner, _, _, _)| *owner == entity);
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut last = after;
        let mut more = false;
        for (owner, kind, schema, payload) in components.by_ref() {
            let key = ((owner.get() as u128) << 32) | kind.get() as u128;
            if key <= after {
                continue;
            }
            let row_bytes = std::mem::size_of::<ComponentView<'_>>() + payload.len();
            if rows.len() >= request.limit()
                || bytes.saturating_add(row_bytes) > request.max_bytes()
            {
                if rows.is_empty() {
                    return Err(CommandViewError::RecordTooLarge);
                }
                more = true;
                break;
            }
            bytes += row_bytes;
            rows.push(ComponentView {
                entity: owner,
                kind,
                schema,
                bytes: payload,
            });
            last = key;
        }
        more |= components
            .any(|(owner, kind, _, _)| ((owner.get() as u128) << 32) | kind.get() as u128 > last);
        let next = more.then(|| {
            self.cursor_issuer().issue_versions(
                CommandViewKind::Components,
                scope,
                revisions[0],
                revisions[1],
                revisions[2],
                last,
            )
        });
        Ok(CommandViewPage::new(rows, next))
    }

    fn component_metadata(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<minimium_gameplay::CommandComponentMetadata>, CommandViewError>
    {
        let instance = self
            .worlds
            .get(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        if !instance.entities.contains(entity) {
            return Err(CommandViewError::UnknownEntity);
        }
        let revisions = [
            instance.entities.component_membership_revision() as u128,
            0,
            0,
        ];
        let scope = CommandViewScope::Entity { world, entity };
        let after = self.validate_cursor(request, CommandViewKind::Components, scope, revisions)?;
        let components = self
            .worlds
            .component_views(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut last = after;
        let mut more = false;
        for (owner, kind, schema, payload) in components.filter(|(owner, _, _, _)| *owner == entity)
        {
            let key = ((owner.get() as u128) << 32) | kind.get() as u128;
            if key <= after {
                continue;
            }
            let row_bytes = if request.max_bytes() <= 32 * 1024 {
                128
            } else {
                std::mem::size_of::<minimium_gameplay::CommandComponentMetadata>()
            };
            if rows.len() >= request.limit()
                || bytes.saturating_add(row_bytes) > request.max_bytes()
            {
                if rows.is_empty() {
                    return Err(CommandViewError::RecordTooLarge);
                }
                more = true;
                break;
            }
            rows.push(minimium_gameplay::CommandComponentMetadata {
                kind,
                schema,
                bytes: payload.len(),
            });
            bytes += row_bytes;
            last = key;
        }
        let next = more.then(|| {
            self.cursor_issuer().issue_versions(
                CommandViewKind::Components,
                scope,
                revisions[0],
                revisions[1],
                revisions[2],
                last,
            )
        });
        Ok(CommandViewPage::new(rows, next))
    }
}
