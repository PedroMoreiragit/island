//! Definições de itens. Todo item do jogo nasce aqui.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemId {
    Wood,
    Thatch,
    Stone,
    Flint,
    StonePickaxe,
    StoneHatchet,
    WoodenSpear,
    Campfire,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Pickaxe,
    Hatchet,
    Spear,
}

impl ItemId {
    pub fn name(self) -> &'static str {
        match self {
            ItemId::Wood => "Madeira",
            ItemId::Thatch => "Palha",
            ItemId::Stone => "Pedra",
            ItemId::Flint => "Sílex",
            ItemId::StonePickaxe => "Picareta de Pedra",
            ItemId::StoneHatchet => "Machado de Pedra",
            ItemId::WoodenSpear => "Lança de Madeira",
            ItemId::Campfire => "Fogueira",
        }
    }

    pub fn max_stack(self) -> u32 {
        match self {
            ItemId::Wood | ItemId::Thatch | ItemId::Stone | ItemId::Flint => 100,
            ItemId::Campfire => 5,
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
}
