use minimium_core::ids::PlayerId;
use minimium_world::world::{cell_for, Cell};
use std::collections::HashMap;

pub(super) fn update_cell_index(
    cells: &mut HashMap<Cell, Vec<PlayerId>>,
    player_cells: &mut HashMap<PlayerId, Cell>,
    id: PlayerId,
    position: [f32; 3],
) {
    let new_cell = cell_for(position);
    if player_cells.get(&id).copied() == Some(new_cell) {
        return;
    }
    let old_cell = player_cells.insert(id, new_cell);
    if old_cell != Some(new_cell) {
        if let Some(old_cell) = old_cell {
            let empty = if let Some(players) = cells.get_mut(&old_cell) {
                players.retain(|player| *player != id);
                players.is_empty()
            } else {
                false
            };
            if empty {
                cells.remove(&old_cell);
            }
        }
        cells.entry(new_cell).or_default().push(id);
    }
}
