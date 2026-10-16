//! Estruturas construídas pelo jogador, alinhadas ao grid de tiles.
//! Cada tile comporta um piso (`Layer::Floor`) e um objeto (`Layer::Object`).

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

use crate::inventory::{Inventory, Stack};
use crate::items::ItemId;
use crate::world::TILE;

pub const STORAGE_SLOTS: usize = 18;
pub const CAMPFIRE_SLOTS: usize = 6;
pub const FORGE_SLOTS: usize = 6;
/// Segundos para a fogueira cozinhar uma carne.
pub const COOK_SECS: f32 = 10.0;
/// Segundos para a forja fundir minério em lingote.
pub const SMELT_SECS: f32 = 8.0;
pub const CAMPFIRE_LIGHT: f32 = 150.0;
pub const FORGE_LIGHT: f32 = 100.0;

/// O que uma estrutura de fogo transforma enquanto acesa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Process {
    pub input: ItemId,
    pub amount: u32,
    pub output: ItemId,
    pub secs: f32,
    pub label: &'static str,
}
/// Distância máxima (jogador → centro do tile) para posicionar.
pub const PLACE_REACH: f32 = TILE * 3.5;
/// Distância máxima até uma estação (ex.: bancada) para usar receitas que dependem dela.
pub const STATION_RANGE: f32 = TILE * 3.0;
/// Distância máxima (jogador → borda da estrutura) para interagir.
pub const INTERACT_REACH: f32 = 20.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructureKind {
    Campfire,
    Forge,
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
            StructureKind::Forge => ItemId::Forge,
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

    /// Estruturas de fogo (queimam combustível e transformam itens).
    pub fn process(self) -> Option<Process> {
        match self {
            StructureKind::Campfire => Some(Process {
                input: ItemId::RawMeat,
                amount: 1,
                output: ItemId::CookedMeat,
                secs: COOK_SECS,
                label: "Cozinhando carne",
            }),
            StructureKind::Forge => Some(Process {
                input: ItemId::MetalOre,
                amount: 2,
                output: ItemId::MetalIngot,
                secs: SMELT_SECS,
                label: "Fundindo metal",
            }),
            _ => None,
        }
    }

    pub fn interactive(self) -> bool {
        matches!(
            self,
            StructureKind::Campfire
                | StructureKind::Forge
                | StructureKind::Workbench
                | StructureKind::StorageBox
                | StructureKind::WoodDoor
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
    /// Segundos restantes do combustível que está queimando agora (fogueira).
    #[serde(default)]
    pub fuel: f32,
    /// Fogueira ligada: queima o combustível do próprio inventário enquanto houver.
    #[serde(default)]
    pub active: bool,
    /// Progresso (segundos) da carne que está cozinhando.
    #[serde(default)]
    pub cook: f32,
    /// Conteúdo (caixa de armazenamento, fogueira).
    #[serde(default)]
    pub storage: Option<Inventory>,
}

/// Segundos de fogo que cada item rende como combustível.
pub fn fuel_value(item: ItemId) -> Option<f32> {
    match item {
        ItemId::Wood => Some(60.0),
        ItemId::Thatch => Some(15.0),
        _ => None,
    }
}

impl Structure {
    pub fn new(kind: StructureKind, tile: (i32, i32)) -> Self {
        Self {
            kind,
            tile,
            open: false,
            fuel: 0.0,
            active: false,
            cook: 0.0,
            storage: storage_slots(kind).map(Inventory::new),
        }
    }

    /// Ajusta estruturas vindas de saves antigos (v0.3: fogueira sem inventário).
    pub fn normalize(&mut self) {
        if self.storage.is_none() {
            self.storage = storage_slots(self.kind).map(Inventory::new);
        }
        if self.kind == StructureKind::Campfire && self.fuel > 0.0 {
            self.active = true;
        }
    }

    pub fn rect(&self) -> Rect {
        tile_rect(self.tile)
    }

    pub fn is_lit(&self) -> bool {
        self.kind.process().is_some() && self.active && self.fuel > 0.0
    }

    pub fn light_radius(&self) -> f32 {
        match self.kind {
            StructureKind::Forge => FORGE_LIGHT,
            _ => CAMPFIRE_LIGHT,
        }
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

    /// Pega a próxima unidade de combustível do inventário, se houver.
    fn refuel(&mut self) -> bool {
        let Some(storage) = self.storage.as_mut() else {
            return false;
        };
        for item in [ItemId::Thatch, ItemId::Wood] {
            if storage.remove(item, 1) {
                self.fuel += fuel_value(item).unwrap_or(0.0);
                return true;
            }
        }
        false
    }

    /// Acende/apaga a fogueira. Acender sem combustível falha.
    pub fn toggle_fire(&mut self) -> bool {
        if self.active {
            self.active = false;
            return true;
        }
        if self.fuel <= 0.0 && !self.refuel() {
            return false;
        }
        self.active = true;
        true
    }

    /// Avança fogueira/forja: queima combustível e transforma itens (carne, minério).
    pub fn tick(&mut self, dt: f32) {
        let Some(process) = self.kind.process() else { return };
        if !self.active {
            return;
        }
        self.fuel -= dt;
        if self.fuel <= 0.0 {
            self.fuel = 0.0;
            if !self.refuel() {
                self.active = false;
                return;
            }
        }
        let Some(storage) = self.storage.as_mut() else { return };
        if storage.count(process.input) < process.amount {
            self.cook = 0.0;
            return;
        }
        self.cook += dt;
        if self.cook < process.secs {
            return;
        }
        self.cook = 0.0;
        storage.remove(process.input, process.amount);
        if storage.insert_stack(Stack::new(process.output, 1)).is_some() {
            // Sem espaço para o resultado: devolve a matéria-prima e espera.
            storage.add(process.input, process.amount);
        }
    }
}

fn storage_slots(kind: StructureKind) -> Option<usize> {
    match kind {
        StructureKind::StorageBox => Some(STORAGE_SLOTS),
        StructureKind::Campfire => Some(CAMPFIRE_SLOTS),
        StructureKind::Forge => Some(FORGE_SLOTS),
        _ => None,
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

    fn fire_with(items: &[(ItemId, u32)]) -> Structure {
        let mut fire = Structure::new(StructureKind::Campfire, (0, 0));
        for &(item, n) in items {
            fire.storage.as_mut().unwrap().add(item, n);
        }
        fire
    }

    #[test]
    fn campfire_needs_fuel_and_burns_it() {
        let mut empty = fire_with(&[]);
        assert!(!empty.toggle_fire());
        assert!(!empty.is_lit());

        let mut fire = fire_with(&[(ItemId::Wood, 2)]);
        assert!(fire.toggle_fire());
        assert!(fire.is_lit());
        assert_eq!(fire.storage.as_ref().unwrap().count(ItemId::Wood), 1);
        for _ in 0..121 {
            fire.tick(1.0);
        }
        assert!(!fire.is_lit(), "apaga quando acaba a madeira");
        assert_eq!(fire.storage.as_ref().unwrap().count(ItemId::Wood), 0);
    }

    #[test]
    fn campfire_cooks_meat() {
        let mut fire = fire_with(&[(ItemId::Wood, 5), (ItemId::RawMeat, 2)]);
        fire.toggle_fire();
        for _ in 0..(COOK_SECS as usize * 2 + 1) {
            fire.tick(1.0);
        }
        let storage = fire.storage.as_ref().unwrap();
        assert_eq!(storage.count(ItemId::CookedMeat), 2);
        assert_eq!(storage.count(ItemId::RawMeat), 0);
    }

    #[test]
    fn forge_smelts_two_ore_into_an_ingot() {
        let mut forge = Structure::new(StructureKind::Forge, (0, 0));
        let storage = forge.storage.as_mut().unwrap();
        storage.add(ItemId::Wood, 3);
        storage.add(ItemId::MetalOre, 5);
        assert!(forge.toggle_fire());
        for _ in 0..(SMELT_SECS as usize * 2 + 1) {
            forge.tick(1.0);
        }
        let storage = forge.storage.as_ref().unwrap();
        assert_eq!(storage.count(ItemId::MetalIngot), 2);
        assert_eq!(storage.count(ItemId::MetalOre), 1, "1 minério sozinho não funde");
        assert_eq!(forge.light_radius(), FORGE_LIGHT);
    }

    #[test]
    fn unlit_campfire_does_not_cook() {
        let mut fire = fire_with(&[(ItemId::Wood, 5), (ItemId::RawMeat, 1)]);
        fire.tick(COOK_SECS * 2.0);
        assert_eq!(fire.storage.as_ref().unwrap().count(ItemId::CookedMeat), 0);
    }

    #[test]
    fn old_campfire_gets_storage_on_normalize() {
        let mut old = Structure::new(StructureKind::Campfire, (0, 0));
        old.storage = None;
        old.fuel = 30.0;
        old.normalize();
        assert!(old.storage.is_some() && old.is_lit());
    }

    #[test]
    fn only_containers_have_storage() {
        assert!(Structure::new(StructureKind::StorageBox, (0, 0)).storage.is_some());
        assert!(Structure::new(StructureKind::Campfire, (0, 0)).storage.is_some());
        assert!(Structure::new(StructureKind::WoodWall, (0, 0)).storage.is_none());
    }
}
