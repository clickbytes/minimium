/// Coarse world-space cell coordinate used for bounded local queries.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Cell(pub i32, pub i32, pub i32);

pub fn cell_for(position: [f32; 3]) -> Cell {
    Cell(
        (position[0] / super::CELL_SIZE).floor() as i32,
        (position[1] / super::CELL_SIZE).floor() as i32,
        (position[2] / super::CELL_SIZE).floor() as i32,
    )
}
