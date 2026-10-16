//! Estruturas construídas pelo jogador, alinhadas ao grid de tiles.
//! Cada tile comporta um piso (`Layer::Floor`) e um objeto (`Layer::Object`).

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

use crate::inventory::Inventory;
use crate::items::ItemId;
use crate::world::TILE;

pub const STORAGE_SLOTS: usize = 18;
/// Segundos de fogo por madeira colocada na fogueira.
pub const FUEL_PER_WOOD: f32 = 60.0;
pub const MAX_FUEL: f32 = 600.0;
pub const CAMPFIRE_LIGHT: f32 = 150.0;
/// Distância máxima (jogador → centro do tile) para posicionar.
pub const PLACE_REACH: f32 = TILE * 3.5;
/// Distância máxima até uma estação (ex.: bancada) para usar receitas que dependem dela.
pub const STATION_RANGE: f32 = TILE * 3.0;
/// Distância máxima (jogador → borda da estrutura) para interagir.
pub const INTERACT_REACH: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureKind {
    Campfire,
    Workbench,
    StorageBox,
    WoodFoundation,
    WoodWall,
    WoodDoor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Floor,
    Object,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    TooFar,
    NotLand,
    Occupied,
    Blocked,
}

impl PlaceError {
    pub fn message(&self) -> &'static str {
        match self {
            PlaceError::TooFar => "Longe demais",
            PlaceError::NotLand => "Só é possível construir em terra firme",
            PlaceError::Occupied => "Já existe algo aqui",
            PlaceError::Blocked => "Algo está bloqueando o lugar",
        }
    }
}

impl StructureKind {
    pub fn item(self) -> ItemId {
        match self {
            StructureKind::Campfire => ItemId::Campfire,
            StructureKind::Workbench => ItemId::Workbench,
            StructureKind::StorageBox => ItemId::StorageBox,
            StructureKind::WoodFoundation => ItemId::WoodFoundation,
            StructureKind::WoodWall => ItemId::WoodWall,
            StructureKind::WoodDoor => ItemId::WoodDoor,
        }
    }

    pub fn layer(self) -> Layer {
        match self {
            StructureKind::WoodFoundation => Layer::Floor,
            _ => Layer::Object,
        }
    }

    pub fn interactive(self) -> bool {
        matches!(
            self,
            StructureKind::Campfire | StructureKind::Workbench | StructureKind::StorageBox | StructureKind::WoodDoor
        )
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    pub kind: StructureKind,
    pub tile: (i32, i32),
    /// Porta aberta.
    #[serde(default)]
    pub open: bool,
    /// Segundos de fogo restantes (fogueira).
    #[serde(default)]
    pub fuel: f32,
    /// Conteúdo (caixa de armazenamento).
    #[serde(default)]
    pub storage: Option<Inventory>,
}

impl Structure {
    pub fn new(kind: StructureKind, tile: (i32, i32)) -> Self {
        Self {
            kind,
            tile,
            open: false,
            fuel: 0.0,
            storage: (kind == StructureKind::StorageBox).then(|| Inventory::new(STORAGE_SLOTS)),
        }
    }

    pub fn rect(&self) -> Rect {
        tile_rect(self.tile)
    }

    pub fn is_lit(&self) -> bool {
        self.kind == StructureKind::Campfire && self.fuel > 0.0
    }

    /// Y usada no y-sort: a base da estrutura.
    pub fn sort_y(&self) -> f32 {
        self.rect().bottom() - 1.0
    }

    /// Um círculo (jogador) colide com esta estrutura?
    pub fn blocks(&self, pos: Vec2, radius: f32) -> bool {
        let r = self.rect();
        match self.kind {
            StructureKind::WoodFoundation => false,
            StructureKind::WoodDoor if self.open => false,
            StructureKind::Campfire => r.center().distance(pos) < radius + 10.0,
            _ => distance_to_rect(pos, r) < radius,
        }
    }

    /// Adiciona combustível; retorna false se a fogueira já está cheia.
    pub fn add_fuel(&mut self) -> bool {
        if self.fuel + FUEL_PER_WOOD > MAX_FUEL {
            return false;
        }
        self.fuel += FUEL_PER_WOOD;
        true
    }
}

pub fn tile_of(pos: Vec2) -> (i32, i32) {
    ((pos.x / TILE).floor() as i32, (pos.y / TILE).floor() as i32)
}

pub fn tile_rect(tile: (i32, i32)) -> Rect {
    Rect::new(tile.0 as f32 * TILE, tile.1 as f32 * TILE, TILE, TILE)
}

/// Distância de um ponto até um retângulo (0 se estiver dentro).
pub fn distance_to_rect(p: Vec2, r: Rect) -> f32 {
    let closest = vec2(p.x.clamp(r.x, r.right()), p.y.clamp(r.y, r.bottom()));
    closest.distance(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_block_and_open_doors_do_not() {
        let wall = Structure::new(StructureKind::WoodWall, (2, 2));
        let center = wall.rect().center();
        assert!(wall.blocks(center, 7.0));
        assert!(wall.blocks(center + vec2(0.0, TILE / 2.0 + 5.0), 7.0));
        assert!(!wall.blocks(center + vec2(0.0, TILE / 2.0 + 8.0), 7.0));

        let mut door = Structure::new(StructureKind::WoodDoor, (2, 2));
        assert!(door.blocks(center, 7.0));
        door.open = true;
        assert!(!door.blocks(center, 7.0));

        let floor = Structure::new(StructureKind::WoodFoundation, (2, 2));
        assert!(!floor.blocks(center, 7.0));
    }

    #[test]
    fn campfire_fuel_is_capped() {
        let mut fire = Structure::new(StructureKind::Campfire, (0, 0));
        assert!(!fire.is_lit());
        let mut added = 0;
        while fire.add_fuel() {
            added += 1;
        }
        assert_eq!(added, (MAX_FUEL / FUEL_PER_WOOD) as i32);
        assert!(fire.is_lit());
    }

    #[test]
    fn only_storage_boxes_have_storage() {
        assert!(Structure::new(StructureKind::StorageBox, (0, 0)).storage.is_some());
        assert!(Structure::new(StructureKind::WoodWall, (0, 0)).storage.is_none());
    }
}
