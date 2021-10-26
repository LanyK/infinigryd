use actlib::api::*;
use crate::messages::PlayerMoves;

type Position = (i64, i64);

#[derive(Debug)]
pub(crate) struct Field {
    players: Vec<()>,
    position: Option<Position>,
}

impl Actor for Field {}

impl Field {
    pub fn new() -> Field {
        Field {
            players: Vec::new(),
            position: None,
        }
    }

    pub fn type_id() -> &'static str {
        return "FIELD";
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

fn handle_player_arrives(field: &mut Field, _msg: &PlayerMoves) {
    
}

impl_message_handler!(Field: PlayerMoves => handle_player_arrives);
