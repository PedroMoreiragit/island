//! Atributos de sobrevivência: vida, comida, água e stamina (estilo ARK).

use serde::{Deserialize, Serialize};

use crate::items::FoodEffect;

/// Máximos base (nível 1, sem pontos de atributo).
pub const MAX_HEALTH: f32 = 100.0;
pub const MAX_FOOD: f32 = 100.0;
pub const MAX_WATER: f32 = 100.0;
pub const MAX_STAMINA: f32 = 100.0;

/// Comida zera em ~10 min, água em ~7 min (parado).
const FOOD_DRAIN: f32 = MAX_FOOD / 600.0;
const WATER_DRAIN: f32 = MAX_WATER / 420.0;
/// Correr gasta comida/água mais rápido.
const SPRINT_NEEDS_MULT: f32 = 2.5;
const SPRINT_STAMINA_DRAIN: f32 = 14.0;
const STAMINA_REGEN: f32 = 18.0;
/// Após esgotar, a stamina precisa voltar até aqui para correr de novo.
const EXHAUSTED_RECOVERY: f32 = 25.0;
/// Dano por segundo para cada necessidade zerada.
const STARVE_DAMAGE: f32 = 1.0;
const HEALTH_REGEN: f32 = 0.5;
/// Vida só regenera com comida e água acima desta fração.
const REGEN_THRESHOLD: f32 = 0.3;

pub const SWING_STAMINA: f32 = 3.0;
pub const DRINK_WATER: f32 = 20.0;

/// Valores máximos de cada atributo (crescem com pontos de atributo).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Maxima {
    pub health: f32,
    pub food: f32,
    pub water: f32,
    pub stamina: f32,
}

impl Default for Maxima {
    fn default() -> Self {
        Self {
            health: MAX_HEALTH,
            food: MAX_FOOD,
            water: MAX_WATER,
            stamina: MAX_STAMINA,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vitals {
    pub health: f32,
    pub food: f32,
    pub water: f32,
    pub stamina: f32,
    pub exhausted: bool,
    /// Saves anteriores à v0.6 não têm máximos: usam os valores base.
    #[serde(default)]
    pub max: Maxima,
}

impl Vitals {
    pub fn full() -> Self {
        Self::full_with(Maxima::default())
    }

    pub fn full_with(max: Maxima) -> Self {
        Self {
            health: max.health,
            food: max.food,
            water: max.water,
            stamina: max.stamina,
            exhausted: false,
            max,
        }
    }

    /// Troca os máximos (ex.: ponto de atributo). O que aumentou também enche.
    pub fn set_max(&mut self, max: Maxima) {
        self.health = (self.health + (max.health - self.max.health).max(0.0)).min(max.health);
        self.food = (self.food + (max.food - self.max.food).max(0.0)).min(max.food);
        self.water = (self.water + (max.water - self.max.water).max(0.0)).min(max.water);
        self.stamina = (self.stamina + (max.stamina - self.max.stamina).max(0.0)).min(max.stamina);
        self.max = max;
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }

    pub fn can_sprint(&self) -> bool {
        !self.exhausted && self.stamina > 0.0
    }

    /// `sprinting` deve ser true apenas se o jogador está de fato correndo (movendo + Shift + `can_sprint`).
    pub fn tick(&mut self, dt: f32, sprinting: bool) {
        let needs_mult = if sprinting { SPRINT_NEEDS_MULT } else { 1.0 };
        self.food = (self.food - FOOD_DRAIN * needs_mult * dt).max(0.0);
        self.water = (self.water - WATER_DRAIN * needs_mult * dt).max(0.0);

        if sprinting {
            self.drain_stamina(SPRINT_STAMINA_DRAIN * dt);
        } else {
            self.stamina = (self.stamina + STAMINA_REGEN * dt).min(self.max.stamina);
            if self.stamina >= EXHAUSTED_RECOVERY {
                self.exhausted = false;
            }
        }

        let starving = [self.food, self.water].iter().filter(|&&v| v <= 0.0).count() as f32;
        if starving > 0.0 {
            self.damage(STARVE_DAMAGE * starving * dt);
        } else if self.food >= self.max.food * REGEN_THRESHOLD && self.water >= self.max.water * REGEN_THRESHOLD {
            self.health = (self.health + HEALTH_REGEN * dt).min(self.max.health);
        }
    }

    fn drain_stamina(&mut self, amount: f32) {
        self.stamina = (self.stamina - amount).max(0.0);
        if self.stamina <= 0.0 {
            self.exhausted = true;
        }
    }

    /// Gasta stamina para uma ação (golpe). Falha sem gastar se não houver o suficiente.
    pub fn spend_stamina(&mut self, amount: f32) -> bool {
        if self.stamina < amount {
            return false;
        }
        self.drain_stamina(amount);
        true
    }

    pub fn damage(&mut self, amount: f32) {
        self.health = (self.health - amount).max(0.0);
    }

    pub fn eat(&mut self, effect: FoodEffect) {
        self.food = (self.food + effect.food).min(self.max.food);
        self.water = (self.water + effect.water).min(self.max.water);
        self.health = (self.health + effect.health).min(self.max.health);
    }

    pub fn drink(&mut self, amount: f32) {
        self.water = (self.water + amount).min(self.max.water);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn needs_drain_and_sprint_costs_more() {
        let (mut walk, mut run) = (Vitals::full(), Vitals::full());
        walk.tick(10.0, false);
        run.tick(1.0, true);
        assert!(walk.food < MAX_FOOD && walk.water < MAX_WATER);
        assert!(run.stamina < MAX_STAMINA);
        let mut run10 = Vitals::full();
        for _ in 0..10 {
            run10.tick(1.0, true);
        }
        assert!(run10.water < walk.water);
    }

    #[test]
    fn starvation_kills() {
        let mut v = Vitals::full();
        v.food = 0.0;
        v.water = 0.0;
        v.tick(49.0, false);
        assert!(!v.is_dead());
        v.tick(2.0, false);
        assert!(v.is_dead());
    }

    #[test]
    fn exhaustion_blocks_sprint_until_recovered() {
        let mut v = Vitals::full();
        v.stamina = 1.0;
        v.tick(1.0, true);
        assert!(!v.can_sprint());
        v.tick(1.0, false); // 18 de stamina: ainda exausto
        assert!(!v.can_sprint());
        v.tick(1.0, false);
        assert!(v.can_sprint());
    }

    #[test]
    fn health_regenerates_when_fed() {
        let mut v = Vitals::full();
        v.health = 50.0;
        v.tick(10.0, false);
        assert!(v.health > 50.0);
    }
}
