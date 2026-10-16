//! Receitas de crafting (valores inspirados no ARK, ajustáveis) e fila de fabricação com tempo.
//! Ingredientes são consumidos ao entrar na fila; o resultado sai quando o tempo termina.

use serde::{Deserialize, Serialize};

use crate::building::StructureKind;
use crate::inventory::{Inventory, Stack};
use crate::items::ItemId;

pub struct Recipe {
    pub output: ItemId,
    pub count: u32,
    pub inputs: &'static [(ItemId, u32)],
    /// Segundos para fabricar uma unidade da receita.
    pub secs: f32,
    /// Estação que precisa estar por perto (ex.: bancada).
    pub station: Option<StructureKind>,
}

const BENCH: Option<StructureKind> = Some(StructureKind::Workbench);

pub const RECIPES: &[Recipe] = &[
    Recipe {
        output: ItemId::StonePickaxe,
        count: 1,
        inputs: &[(ItemId::Wood, 1), (ItemId::Stone, 1), (ItemId::Thatch, 10)],
        secs: 2.0,
        station: None,
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
        secs: 2.0,
        station: None,
    },
    Recipe {
        output: ItemId::WoodenSpear,
        count: 1,
        inputs: &[(ItemId::Wood, 8), (ItemId::Thatch, 12)],
        secs: 2.5,
        station: None,
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
        secs: 3.0,
        station: None,
    },
    Recipe {
        output: ItemId::Workbench,
        count: 1,
        inputs: &[(ItemId::Wood, 20), (ItemId::Stone, 10), (ItemId::Thatch, 10)],
        secs: 5.0,
        station: None,
    },
    Recipe {
        output: ItemId::StorageBox,
        count: 1,
        inputs: &[(ItemId::Wood, 25), (ItemId::Thatch, 15)],
        secs: 4.0,
        station: None,
    },
    Recipe {
        output: ItemId::WoodFoundation,
        count: 1,
        inputs: &[(ItemId::Wood, 8), (ItemId::Stone, 2)],
        secs: 2.0,
        station: BENCH,
    },
    Recipe {
        output: ItemId::WoodWall,
        count: 1,
        inputs: &[(ItemId::Wood, 6), (ItemId::Thatch, 2)],
        secs: 2.0,
        station: BENCH,
    },
    Recipe {
        output: ItemId::WoodDoor,
        count: 1,
        inputs: &[(ItemId::Wood, 5), (ItemId::Thatch, 3)],
        secs: 2.0,
        station: BENCH,
    },
];

pub const MAX_QUEUE: usize = 8;

pub fn recipe_for(output: ItemId) -> Option<&'static Recipe> {
    RECIPES.iter().find(|r| r.output == output)
}

#[derive(Debug, PartialEq, Eq)]
pub enum CraftError {
    MissingIngredients,
    NeedsStation(StructureKind),
    QueueFull,
}

pub fn can_afford(inv: &Inventory, recipe: &Recipe) -> bool {
    recipe.inputs.iter().all(|&(item, n)| inv.count(item) >= n)
}

/// Um item em fabricação. Guardamos o item produzido (não o índice) para o save sobreviver a
/// mudanças na ordem das receitas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CraftJob {
    pub output: ItemId,
    pub progress: f32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CraftQueue {
    pub jobs: Vec<CraftJob>,
}

impl CraftQueue {
    /// Consome os ingredientes e coloca a receita no fim da fila.
    pub fn enqueue(&mut self, inv: &mut Inventory, recipe: &Recipe, station_near: bool) -> Result<(), CraftError> {
        if self.jobs.len() >= MAX_QUEUE {
            return Err(CraftError::QueueFull);
        }
        if let Some(station) = recipe.station
            && !station_near
        {
            return Err(CraftError::NeedsStation(station));
        }
        if !can_afford(inv, recipe) {
            return Err(CraftError::MissingIngredients);
        }
        for &(item, n) in recipe.inputs {
            inv.remove(item, n);
        }
        self.jobs.push(CraftJob {
            output: recipe.output,
            progress: 0.0,
        });
        Ok(())
    }

    /// Avança a fila; devolve as pilhas prontas.
    pub fn tick(&mut self, dt: f32) -> Vec<Stack> {
        let mut done = Vec::new();
        let mut budget = dt;
        while let Some(job) = self.jobs.first_mut() {
            let Some(recipe) = recipe_for(job.output) else {
                self.jobs.remove(0);
                continue;
            };
            let missing = recipe.secs - job.progress;
            if budget < missing {
                job.progress += budget;
                break;
            }
            budget -= missing;
            done.push(Stack::new(recipe.output, recipe.count));
            self.jobs.remove(0);
        }
        done
    }

    /// Cancela um item da fila e devolve os ingredientes. Retorna o que não coube no inventário.
    pub fn cancel(&mut self, index: usize, inv: &mut Inventory) -> Vec<Stack> {
        if index >= self.jobs.len() {
            return Vec::new();
        }
        let job = self.jobs.remove(index);
        let Some(recipe) = recipe_for(job.output) else {
            return Vec::new();
        };
        recipe
            .inputs
            .iter()
            .filter_map(|&(item, n)| inv.insert_stack(Stack::new(item, n)))
            .collect()
    }

    /// Progresso (0..1) do item sendo fabricado agora.
    pub fn current_progress(&self) -> Option<f32> {
        let job = self.jobs.first()?;
        let recipe = recipe_for(job.output)?;
        Some((job.progress / recipe.secs).clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pickaxe_materials() -> Inventory {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 2);
        inv.add(ItemId::Stone, 1);
        inv.add(ItemId::Thatch, 10);
        inv
    }

    #[test]
    fn queue_consumes_then_delivers_after_time() {
        let mut inv = pickaxe_materials();
        let mut queue = CraftQueue::default();
        let recipe = recipe_for(ItemId::StonePickaxe).unwrap();
        assert_eq!(queue.enqueue(&mut inv, recipe, false), Ok(()));
        assert_eq!(inv.count(ItemId::Thatch), 0);
        assert!(queue.tick(recipe.secs - 0.5).is_empty());
        assert!(queue.current_progress().unwrap() > 0.5);
        let done = queue.tick(1.0);
        assert_eq!(done, vec![Stack::new(ItemId::StonePickaxe, 1)]);
        assert!(queue.jobs.is_empty());
    }

    #[test]
    fn one_big_tick_finishes_several_jobs() {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 20);
        inv.add(ItemId::Thatch, 30);
        let mut queue = CraftQueue::default();
        let spear = recipe_for(ItemId::WoodenSpear).unwrap();
        queue.enqueue(&mut inv, spear, false).unwrap();
        queue.enqueue(&mut inv, spear, false).unwrap();
        assert_eq!(queue.tick(spear.secs * 2.0 + 0.1).len(), 2);
    }

    #[test]
    fn missing_ingredients_or_station_changes_nothing() {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 1);
        let mut queue = CraftQueue::default();
        let pickaxe = recipe_for(ItemId::StonePickaxe).unwrap();
        assert_eq!(
            queue.enqueue(&mut inv, pickaxe, true),
            Err(CraftError::MissingIngredients)
        );

        inv.add(ItemId::Wood, 20);
        inv.add(ItemId::Thatch, 10);
        let wall = recipe_for(ItemId::WoodWall).unwrap();
        assert_eq!(
            queue.enqueue(&mut inv, wall, false),
            Err(CraftError::NeedsStation(StructureKind::Workbench))
        );
        assert_eq!(inv.count(ItemId::Wood), 21);
        assert_eq!(queue.enqueue(&mut inv, wall, true), Ok(()));
    }

    #[test]
    fn cancel_refunds_ingredients() {
        let mut inv = pickaxe_materials();
        let mut queue = CraftQueue::default();
        queue
            .enqueue(&mut inv, recipe_for(ItemId::StonePickaxe).unwrap(), false)
            .unwrap();
        assert!(queue.cancel(0, &mut inv).is_empty());
        assert_eq!(inv.count(ItemId::Thatch), 10);
        assert_eq!(inv.count(ItemId::Wood), 2);
        assert!(queue.jobs.is_empty());
    }

    #[test]
    fn queue_has_a_limit() {
        let mut inv = Inventory::new(10);
        inv.add(ItemId::Wood, 100);
        inv.add(ItemId::Thatch, 100);
        let mut queue = CraftQueue::default();
        let spear = recipe_for(ItemId::WoodenSpear).unwrap();
        for _ in 0..MAX_QUEUE {
            queue.enqueue(&mut inv, spear, false).unwrap();
        }
        assert_eq!(queue.enqueue(&mut inv, spear, false), Err(CraftError::QueueFull));
    }

    #[test]
    fn every_recipe_output_is_unique() {
        for r in RECIPES {
            assert_eq!(RECIPES.iter().filter(|o| o.output == r.output).count(), 1);
        }
    }
}
