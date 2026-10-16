//! Receitas de crafting (valores inspirados no ARK, ajustáveis).

use crate::inventory::Inventory;
use crate::items::ItemId;

pub struct Recipe {
    pub output: ItemId,
    pub count: u32,
    pub inputs: &'static [(ItemId, u32)],
}

pub const RECIPES: &[Recipe] = &[
    Recipe {
        output: ItemId::StonePickaxe,
        count: 1,
        inputs: &[(ItemId::Wood, 1), (ItemId::Stone, 1), (ItemId::Thatch, 10)],
    },
    Recipe {
        output: ItemId::StoneHatchet,
        count: 1,
        inputs: &[
            (ItemId::Wood, 1),
            (ItemId::Flint, 1),
            (ItemId::Stone, 1),
            (ItemId::Thatch, 10),
        ],
    },
    Recipe {
        output: ItemId::WoodenSpear,
        count: 1,
        inputs: &[(ItemId::Wood, 8), (ItemId::Thatch, 12)],
    },
    Recipe {
        output: ItemId::Campfire,
        count: 1,
        inputs: &[
            (ItemId::Stone, 16),
            (ItemId::Wood, 2),
            (ItemId::Thatch, 12),
            (ItemId::Flint, 1),
        ],
    },
];

#[derive(Debug, PartialEq, Eq)]
pub enum CraftError {
    MissingIngredients,
    InventoryFull,
}

pub fn can_afford(inv: &Inventory, recipe: &Recipe) -> bool {
    recipe.inputs.iter().all(|&(item, n)| inv.count(item) >= n)
}

/// Consome ingredientes e entrega o resultado de forma atômica.
pub fn craft(inv: &mut Inventory, recipe: &Recipe) -> Result<(), CraftError> {
    if !can_afford(inv, recipe) {
        return Err(CraftError::MissingIngredients);
    }
    let mut next = inv.clone();
    for &(item, n) in recipe.inputs {
        next.remove(item, n);
    }
    if next.add(recipe.output, recipe.count) > 0 {
        return Err(CraftError::InventoryFull);
    }
    *inv = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crafts_pickaxe() {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 2);
        inv.add(ItemId::Stone, 1);
        inv.add(ItemId::Thatch, 10);
        assert_eq!(craft(&mut inv, &RECIPES[0]), Ok(()));
        assert_eq!(inv.count(ItemId::StonePickaxe), 1);
        assert_eq!(inv.count(ItemId::Wood), 1);
        assert_eq!(inv.count(ItemId::Thatch), 0);
    }

    #[test]
    fn missing_ingredients_changes_nothing() {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 1);
        assert_eq!(craft(&mut inv, &RECIPES[0]), Err(CraftError::MissingIngredients));
        assert_eq!(inv.count(ItemId::Wood), 1);
    }

    #[test]
    fn full_inventory_rolls_back() {
        // 3 slots: madeira, pedra e palha ainda sobrando -> nenhum slot livre para a picareta.
        let mut inv = Inventory::new(3);
        inv.add(ItemId::Wood, 5);
        inv.add(ItemId::Stone, 5);
        inv.add(ItemId::Thatch, 20);
        assert_eq!(craft(&mut inv, &RECIPES[0]), Err(CraftError::InventoryFull));
        assert_eq!(inv.count(ItemId::Thatch), 20);
    }
}
