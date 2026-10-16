//! O que cada golpe rende, por tipo de nó e ferramenta.
//! Como no ARK: machado favorece madeira/pedra, picareta favorece palha/sílex.

use crate::items::{ItemId, ToolKind};
use crate::rng::Rng;
use crate::world::NodeKind;

pub fn roll_yield(kind: NodeKind, tool: Option<ToolKind>, rng: &mut Rng) -> Vec<(ItemId, u32)> {
    let mut drops = Vec::new();
    let mut push = |item, n| {
        if n > 0 {
            drops.push((item, n));
        }
    };
    // Lança não é ferramenta de coleta: conta como mão.
    let tool = tool.filter(|t| *t != ToolKind::Spear);
    match (kind, tool) {
        (NodeKind::Pebble, _) => push(ItemId::Stone, 1),
        (NodeKind::Tree, None) => {
            push(ItemId::Wood, 1);
            push(ItemId::Thatch, rng.range_u32(1, 2));
        }
        (NodeKind::Tree, Some(ToolKind::Hatchet)) => {
            push(ItemId::Wood, rng.range_u32(3, 4));
            push(ItemId::Thatch, u32::from(rng.chance(0.3)));
        }
        (NodeKind::Tree, Some(_)) => {
            push(ItemId::Wood, 1);
            push(ItemId::Thatch, rng.range_u32(3, 4));
        }
        (NodeKind::Rock, None) => {
            push(ItemId::Stone, 1);
            push(ItemId::Flint, u32::from(rng.chance(0.2)));
        }
        (NodeKind::Rock, Some(ToolKind::Hatchet)) => {
            push(ItemId::Stone, rng.range_u32(3, 4));
            push(ItemId::Flint, u32::from(rng.chance(0.3)));
        }
        (NodeKind::Rock, Some(_)) => {
            push(ItemId::Stone, 1);
            push(ItemId::Flint, rng.range_u32(2, 3));
        }
    }
    drops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(drops: &[(ItemId, u32)], item: ItemId) -> u32 {
        drops.iter().filter(|d| d.0 == item).map(|d| d.1).sum()
    }

    #[test]
    fn tools_beat_bare_hands() {
        let mut rng = Rng::new(1);
        for _ in 0..50 {
            let hand = roll_yield(NodeKind::Tree, None, &mut rng);
            let hatchet = roll_yield(NodeKind::Tree, Some(ToolKind::Hatchet), &mut rng);
            assert!(total(&hatchet, ItemId::Wood) > total(&hand, ItemId::Wood));
            let pick = roll_yield(NodeKind::Rock, Some(ToolKind::Pickaxe), &mut rng);
            assert!(total(&pick, ItemId::Flint) >= 2);
        }
    }
}
