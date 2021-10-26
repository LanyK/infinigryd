use serde::{Deserialize, Serialize};

/// Specify a Direction.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Clone)]
pub enum Direction {
    North,
    West,
    South,
    East,
}

impl Direction {
    /// return the opposite direction
    pub fn reverse(&self) -> Direction {
        match self {
            Direction::North => Direction::South,
            Direction::West => Direction::East,
            Direction::South => Direction::North,
            Direction::East => Direction::West,
        }
    }
}

pub const DIRECTIONS: [Direction; 4] = [
    Direction::North,
    Direction::West,
    Direction::South,
    Direction::East,
];

/// The Position of a field in the infinite Grid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub(crate) x: i64,
    pub(crate) y: i64,
}

impl Position {
    pub fn next(&self, direction: &Direction) -> Position {
        match direction {
            Direction::North => Position {
                x: self.x,
                y: self.y + 1,
            },
            Direction::West => Position {
                x: self.x - 1,
                y: self.y,
            },
            Direction::South => Position {
                x: self.x,
                y: self.y - 1,
            },
            Direction::East => Position {
                x: self.x + 1,
                y: self.y,
            },
        }
    }
}
