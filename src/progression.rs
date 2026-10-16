//! Progressão do jogador: XP, nível, pontos de atributo e engramas (receitas desbloqueadas).

use serde::{Deserialize, Serialize};

use crate::crafting::{RECIPES, Recipe};
use crate::items::ItemId;
use crate::vitals::Maxima;

pub const MAX_LEVEL: u32 = 30;
pub const ENGRAM_POINTS_PER_LEVEL: u32 = 8;
/// Quanto cada ponto de atributo soma ao máximo (vida, stamina, comida, água).
pub const POINTS_TO_STAT: f32 = 10.0;
/// Dano corpo a corpo extra por ponto (fração).
pub const MELEE_PER_POINT: f32 = 0.05;

/// XP por golpe de coleta (nó ou carcaça) que rendeu algo.
pub const XP_GATHER: u32 = 4;
/// XP por segundo de receita fabricada.
pub const XP_CRAFT_PER_SEC: f32 = 3.0;

pub fn xp_kill(creature_level: u32) -> u32 {
    10 + 3 * creature_level
}

pub fn xp_tame(creature_level: u32) -> u32 {
    40 + 6 * creature_level
}

/// XP necessária para sair de `level` e ir para o próximo.
pub fn xp_to_next(level: u32) -> u32 {
    25 + 6 * level * level
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Attribute {
    Health,
    Stamina,
    Food,
    Water,
    Melee,
}

impl Attribute {
    pub const ALL: [Attribute; 5] = [
        Attribute::Health,
        Attribute::Stamina,
        Attribute::Food,
        Attribute::Water,
        Attribute::Melee,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Attribute::Health => "Vida",
            Attribute::Stamina => "Stamina",
            Attribute::Food => "Comida",
            Attribute::Water => "Água",
            Attribute::Melee => "Dano",
        }
    }
}

/// Pontos gastos em cada atributo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attributes {
    pub health: u32,
    pub stamina: u32,
    pub food: u32,
    pub water: u32,
    pub melee: u32,
}

impl Attributes {
    pub fn get(&self, attr: Attribute) -> u32 {
        match attr {
            Attribute::Health => self.health,
            Attribute::Stamina => self.stamina,
            Attribute::Food => self.food,
            Attribute::Water => self.water,
            Attribute::Melee => self.melee,
        }
    }

    fn get_mut(&mut self, attr: Attribute) -> &mut u32 {
        match attr {
            Attribute::Health => &mut self.health,
            Attribute::Stamina => &mut self.stamina,
            Attribute::Food => &mut self.food,
            Attribute::Water => &mut self.water,
            Attribute::Melee => &mut self.melee,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum LearnError {
    AlreadyKnown,
    LevelTooLow(u32),
    NotEnoughPoints(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Progression {
    pub level: u32,
    /// XP acumulada dentro do nível atual.
    pub xp: u32,
    pub attribute_points: u32,
    pub engram_points: u32,
    pub attributes: Attributes,
    /// Receitas aprendidas, pelo item que produzem.
    pub known: Vec<ItemId>,
}

impl Default for Progression {
    fn default() -> Self {
        Self {
            level: 1,
            xp: 0,
            attribute_points: 0,
            engram_points: 0,
            attributes: Attributes::default(),
            known: RECIPES
                .iter()
                .filter(|r| r.engram_cost == 0)
                .map(|r| r.output)
                .collect(),
        }
    }
}

impl Progression {
    /// Para saves anteriores à v0.6: tudo o que já existia fica liberado (só o tier de metal é novo).
    pub fn legacy() -> Self {
        Self {
            known: RECIPES.iter().filter(|r| r.level < 10).map(|r| r.output).collect(),
            ..Self::default()
        }
    }

    /// Soma XP. Retorna quantos níveis foram ganhos.
    pub fn add_xp(&mut self, amount: u32) -> u32 {
        if self.level >= MAX_LEVEL {
            return 0;
        }
        self.xp += amount;
        let mut gained = 0;
        while self.level < MAX_LEVEL && self.xp >= xp_to_next(self.level) {
            self.xp -= xp_to_next(self.level);
            self.level += 1;
            self.attribute_points += 1;
            self.engram_points += ENGRAM_POINTS_PER_LEVEL;
            gained += 1;
        }
        if self.level >= MAX_LEVEL {
            self.xp = 0;
        }
        gained
    }

    /// Fração (0..1) da XP até o próximo nível.
    pub fn xp_frac(&self) -> f32 {
        if self.level >= MAX_LEVEL {
            return 1.0;
        }
        self.xp as f32 / xp_to_next(self.level) as f32
    }

    pub fn spend(&mut self, attr: Attribute) -> bool {
        if self.attribute_points == 0 {
            return false;
        }
        self.attribute_points -= 1;
        *self.attributes.get_mut(attr) += 1;
        true
    }

    pub fn maxima(&self) -> Maxima {
        let base = Maxima::default();
        let bonus = |points: u32| points as f32 * POINTS_TO_STAT;
        Maxima {
            health: base.health + bonus(self.attributes.health),
            stamina: base.stamina + bonus(self.attributes.stamina),
            food: base.food + bonus(self.attributes.food),
            water: base.water + bonus(self.attributes.water),
        }
    }

    pub fn melee_mult(&self) -> f32 {
        1.0 + MELEE_PER_POINT * self.attributes.melee as f32
    }

    pub fn knows(&self, item: ItemId) -> bool {
        self.known.contains(&item)
    }

    pub fn can_learn(&self, recipe: &Recipe) -> Result<(), LearnError> {
        if self.knows(recipe.output) {
            return Err(LearnError::AlreadyKnown);
        }
        if self.level < recipe.level {
            return Err(LearnError::LevelTooLow(recipe.level));
        }
        if self.engram_points < recipe.engram_cost {
            return Err(LearnError::NotEnoughPoints(recipe.engram_cost));
        }
        Ok(())
    }

    pub fn learn(&mut self, recipe: &Recipe) -> Result<(), LearnError> {
        self.can_learn(recipe)?;
        self.engram_points -= recipe.engram_cost;
        self.known.push(recipe.output);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crafting::recipe_for;

    #[test]
    fn xp_levels_up_and_grants_points() {
        let mut p = Progression::default();
        assert_eq!(p.add_xp(xp_to_next(1) - 1), 0);
        assert_eq!(p.add_xp(1), 1);
        assert_eq!((p.level, p.xp), (2, 0));
        assert_eq!(p.attribute_points, 1);
        assert_eq!(p.engram_points, ENGRAM_POINTS_PER_LEVEL);
        // Muita XP de uma vez sobe vários níveis.
        assert!(p.add_xp(10_000) > 1);
    }

    #[test]
    fn level_is_capped() {
        let mut p = Progression::default();
        p.add_xp(u32::MAX / 2);
        assert_eq!(p.level, MAX_LEVEL);
        assert_eq!(p.add_xp(1000), 0);
        assert_eq!(p.xp_frac(), 1.0);
    }

    #[test]
    fn attributes_raise_maxima_and_damage() {
        let mut p = Progression::default();
        assert!(!p.spend(Attribute::Health), "sem pontos");
        p.attribute_points = 3;
        assert!(p.spend(Attribute::Health));
        assert!(p.spend(Attribute::Health));
        assert!(p.spend(Attribute::Melee));
        assert_eq!(p.maxima().health, Maxima::default().health + 2.0 * POINTS_TO_STAT);
        assert!(p.melee_mult() > 1.0);
    }

    #[test]
    fn engrams_need_level_and_points() {
        let mut p = Progression::default();
        assert!(p.knows(ItemId::StonePickaxe), "receitas iniciais já vêm aprendidas");
        let forge = recipe_for(ItemId::Forge).unwrap();
        assert_eq!(p.learn(forge), Err(LearnError::LevelTooLow(forge.level)));
        p.level = forge.level;
        assert_eq!(p.learn(forge), Err(LearnError::NotEnoughPoints(forge.engram_cost)));
        p.engram_points = forge.engram_cost;
        assert_eq!(p.learn(forge), Ok(()));
        assert_eq!(p.engram_points, 0);
        assert_eq!(p.learn(forge), Err(LearnError::AlreadyKnown));
    }

    #[test]
    fn legacy_saves_keep_old_recipes_but_not_metal() {
        let p = Progression::legacy();
        assert!(p.knows(ItemId::WoodWall) && p.knows(ItemId::RaptorSaddle));
        assert!(!p.knows(ItemId::Forge) && !p.knows(ItemId::MetalPickaxe));
    }
}
