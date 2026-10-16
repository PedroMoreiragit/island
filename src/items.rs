//! Definições de itens. Todo item do jogo nasce aqui.

use serde::{Deserialize, Serialize};

use crate::building::StructureKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ItemId {
    Wood,
    Thatch,
    Stone,
    Flint,
    Berry,
    StonePickaxe,
    StoneHatchet,
    WoodenSpear,
    Campfire,
    Workbench,
    StorageBox,
    WoodFoundation,
    WoodWall,
    WoodDoor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Pickaxe,
    Hatchet,
    Spear,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoodEffect {
    pub food: f32,
    pub water: f32,
    pub health: f32,
}

impl ItemId {
    #[cfg(test)]
    pub const ALL: [ItemId; 14] = [
        ItemId::Wood,
        ItemId::Thatch,
        ItemId::Stone,
        ItemId::Flint,
        ItemId::Berry,
        ItemId::StonePickaxe,
        ItemId::StoneHatchet,
        ItemId::WoodenSpear,
        ItemId::Campfire,
        ItemId::Workbench,
        ItemId::StorageBox,
        ItemId::WoodFoundation,
        ItemId::WoodWall,
        ItemId::WoodDoor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ItemId::Wood => "Madeira",
            ItemId::Thatch => "Palha",
            ItemId::Stone => "Pedra",
            ItemId::Flint => "Sílex",
            ItemId::Berry => "Frutinha",
            ItemId::StonePickaxe => "Picareta de Pedra",
            ItemId::StoneHatchet => "Machado de Pedra",
            ItemId::WoodenSpear => "Lança de Madeira",
            ItemId::Campfire => "Fogueira",
            ItemId::Workbench => "Bancada",
            ItemId::StorageBox => "Caixa de Armazenamento",
            ItemId::WoodFoundation => "Fundação de Madeira",
            ItemId::WoodWall => "Parede de Madeira",
            ItemId::WoodDoor => "Porta de Madeira",
        }
    }

    pub fn max_stack(self) -> u32 {
        match self {
            ItemId::Wood | ItemId::Thatch | ItemId::Stone | ItemId::Flint | ItemId::Berry => 100,
            ItemId::Campfire | ItemId::Workbench | ItemId::StorageBox => 5,
            ItemId::WoodFoundation | ItemId::WoodWall | ItemId::WoodDoor => 20,
            ItemId::StonePickaxe | ItemId::StoneHatchet | ItemId::WoodenSpear => 1,
        }
    }

    pub fn tool(self) -> Option<ToolKind> {
        match self {
            ItemId::StonePickaxe => Some(ToolKind::Pickaxe),
            ItemId::StoneHatchet => Some(ToolKind::Hatchet),
            ItemId::WoodenSpear => Some(ToolKind::Spear),
            _ => None,
        }
    }

    /// Estrutura que o item vira ao ser posicionado no mundo.
    pub fn structure(self) -> Option<StructureKind> {
        match self {
            ItemId::Campfire => Some(StructureKind::Campfire),
            ItemId::Workbench => Some(StructureKind::Workbench),
            ItemId::StorageBox => Some(StructureKind::StorageBox),
            ItemId::WoodFoundation => Some(StructureKind::WoodFoundation),
            ItemId::WoodWall => Some(StructureKind::WoodWall),
            ItemId::WoodDoor => Some(StructureKind::WoodDoor),
            _ => None,
        }
    }

    /// Usos até quebrar (só itens com `max_stack == 1`).
    pub fn max_durability(self) -> Option<u32> {
        match self {
            ItemId::StonePickaxe | ItemId::StoneHatchet => Some(100),
            ItemId::WoodenSpear => Some(50),
            _ => None,
        }
    }

    pub fn food_effect(self) -> Option<FoodEffect> {
        match self {
            ItemId::Berry => Some(FoodEffect {
                food: 8.0,
                water: 2.0,
                health: 0.0,
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_items_never_stack() {
        for item in ItemId::ALL {
            if item.max_durability().is_some() {
                assert_eq!(item.max_stack(), 1, "{item:?}");
            }
        }
    }

    #[test]
    fn structures_round_trip_to_items() {
        for item in ItemId::ALL {
            if let Some(kind) = item.structure() {
                assert_eq!(kind.item(), item);
            }
        }
    }
}
