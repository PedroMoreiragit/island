//! Fauna: espécies, IA simples (parado, vagando, fugindo, perseguindo) e spawn por bioma.
//! Criaturas mortas viram carcaças que o jogador coleta como um nó de recurso.

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};

use crate::rng::Rng;
use crate::world::{TILE, Tile, World};

/// Criaturas longe do jogador ficam congeladas para economizar processamento.
const SIMULATION_RADIUS: f32 = 900.0;
/// Agressivos desistem da perseguição se o jogador se afastar além de `aggro * LEASH`.
const LEASH: f32 = 1.8;
const CORPSE_SECS: f32 = 120.0;
const FLEE_SECS: f32 = 4.0;
/// Agressivos não nascem perto do spawn da praia nem de construções.
const SAFE_ZONE: f32 = TILE * 20.0;
const BASE_SAFE_ZONE: f32 = TILE * 8.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Species {
    Dodo,
    Trilobite,
    Dilophosaur,
    Raptor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Temperament {
    Passive,
    Aggressive,
}

impl Species {
    pub const ALL: [Species; 4] = [Species::Dodo, Species::Trilobite, Species::Dilophosaur, Species::Raptor];

    pub fn name(self) -> &'static str {
        match self {
            Species::Dodo => "Dodô",
            Species::Trilobite => "Trilobita",
            Species::Dilophosaur => "Dilofossauro",
            Species::Raptor => "Raptor",
        }
    }

    pub fn temperament(self) -> Temperament {
        match self {
            Species::Dodo | Species::Trilobite => Temperament::Passive,
            Species::Dilophosaur | Species::Raptor => Temperament::Aggressive,
        }
    }

    pub fn max_health(self) -> f32 {
        match self {
            Species::Dodo => 30.0,
            Species::Trilobite => 25.0,
            Species::Dilophosaur => 60.0,
            Species::Raptor => 90.0,
        }
    }

    /// (andando, correndo) em pixels/s. O jogador anda a 130 e corre a 208.
    pub fn speeds(self) -> (f32, f32) {
        match self {
            Species::Dodo => (40.0, 120.0),
            Species::Trilobite => (20.0, 45.0),
            Species::Dilophosaur => (50.0, 115.0),
            Species::Raptor => (60.0, 170.0),
        }
    }

    pub fn radius(self) -> f32 {
        match self {
            Species::Dodo => 8.0,
            Species::Trilobite => 8.0,
            Species::Dilophosaur => 11.0,
            Species::Raptor => 12.0,
        }
    }

    /// (dano, intervalo entre ataques em s, alcance da borda até o jogador, distância de aggro).
    pub fn attack(self) -> Option<(f32, f32, f32, f32)> {
        match self {
            Species::Dilophosaur => Some((8.0, 1.4, 12.0, 160.0)),
            Species::Raptor => Some((12.0, 1.1, 12.0, 220.0)),
            _ => None,
        }
    }

    /// Golpes de coleta que a carcaça aguenta.
    pub fn harvest_hits(self) -> u32 {
        match self {
            Species::Dodo | Species::Trilobite => 3,
            Species::Dilophosaur => 5,
            Species::Raptor => 6,
        }
    }

    /// Animais maiores rendem mais carne/couro.
    pub fn is_big(self) -> bool {
        matches!(self, Species::Dilophosaur | Species::Raptor)
    }

    /// Limite de população viva na ilha.
    pub fn population_cap(self) -> usize {
        match self {
            Species::Dodo => 25,
            Species::Trilobite => 15,
            Species::Dilophosaur => 10,
            Species::Raptor => 6,
        }
    }

    /// Bioma onde a espécie nasce.
    fn habitat(self, world: &World, x: i32, y: i32) -> bool {
        let tile = world.tile(x, y);
        match self {
            Species::Trilobite => tile == Tile::Sand,
            Species::Dodo => tile == Tile::Grass,
            Species::Dilophosaur => tile == Tile::Grass && world.forest(x, y) > 0.45,
            Species::Raptor => tile == Tile::Grass && world.forest(x, y) > 0.55,
        }
    }

    /// Onde a criatura pode pisar: terra firme (trilobitas também andam na água rasa).
    fn can_stand(self, world: &World, pos: Vec2) -> bool {
        let tile_ok = match world.tile_at(pos) {
            Tile::ShallowWater => self == Species::Trilobite,
            tile => tile.walkable(),
        };
        tile_ok && !world.blocked(pos, self.radius())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Idle,
    Wander,
    Flee,
    Chase,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Corpse {
    pub harvest_left: u32,
    pub ttl: f32,
}

#[derive(Clone, Debug)]
pub struct Creature {
    pub species: Species,
    pub pos: Vec2,
    pub health: f32,
    pub facing_left: bool,
    pub state: State,
    state_timer: f32,
    wander_target: Vec2,
    attack_cooldown: f32,
    /// Animação de bote (s restantes).
    pub lunge: f32,
    /// Piscar vermelho ao levar dano (s restantes).
    pub hurt: f32,
    pub moving: bool,
    pub anim_time: f32,
    pub corpse: Option<Corpse>,
}

/// O que a IA sabe sobre o jogador neste frame.
pub struct AiInput {
    pub player_pos: Vec2,
    pub player_alive: bool,
}

impl Creature {
    pub fn new(species: Species, pos: Vec2) -> Self {
        Self {
            species,
            pos,
            health: species.max_health(),
            facing_left: false,
            state: State::Idle,
            state_timer: 1.0,
            wander_target: pos,
            attack_cooldown: 0.0,
            lunge: 0.0,
            hurt: 0.0,
            moving: false,
            anim_time: 0.0,
            corpse: None,
        }
    }

    pub fn is_alive(&self) -> bool {
        self.corpse.is_none()
    }

    pub fn health_frac(&self) -> f32 {
        (self.health / self.species.max_health()).clamp(0.0, 1.0)
    }

    /// Aplica dano vindo de `from`. Retorna true se a criatura morreu agora.
    pub fn take_hit(&mut self, damage: f32, from: Vec2) -> bool {
        if !self.is_alive() {
            return false;
        }
        self.health -= damage;
        self.hurt = 0.2;
        self.facing_left = from.x < self.pos.x;
        if self.health <= 0.0 {
            self.health = 0.0;
            self.state = State::Idle;
            self.corpse = Some(Corpse {
                harvest_left: self.species.harvest_hits(),
                ttl: CORPSE_SECS,
            });
            return true;
        }
        match self.species.temperament() {
            Temperament::Passive => {
                self.state = State::Flee;
                self.state_timer = FLEE_SECS;
            }
            Temperament::Aggressive => self.state = State::Chase,
        }
        false
    }

    /// Um golpe de coleta na carcaça. Retorna false se não há mais o que coletar.
    pub fn harvest(&mut self) -> bool {
        let Some(corpse) = self.corpse.as_mut() else {
            return false;
        };
        if corpse.harvest_left == 0 {
            return false;
        }
        corpse.harvest_left -= 1;
        self.hurt = 0.15;
        true
    }

    /// Carcaça esgotada ou expirada: pode ser removida.
    pub fn is_gone(&self) -> bool {
        self.corpse.is_some_and(|c| c.harvest_left == 0 || c.ttl <= 0.0)
    }

    /// Avança IA e movimento. Retorna o dano causado ao jogador neste frame.
    pub fn update(&mut self, world: &World, input: &AiInput, rng: &mut Rng, dt: f32) -> f32 {
        self.hurt = (self.hurt - dt).max(0.0);
        self.lunge = (self.lunge - dt).max(0.0);
        if let Some(corpse) = self.corpse.as_mut() {
            corpse.ttl -= dt;
            return 0.0;
        }
        let to_player = input.player_pos - self.pos;
        let dist = to_player.length();
        if dist > SIMULATION_RADIUS {
            self.moving = false;
            return 0.0;
        }
        self.attack_cooldown = (self.attack_cooldown - dt).max(0.0);
        self.state_timer -= dt;
        self.moving = false;

        if let Some((_, _, _, aggro)) = self.species.attack() {
            let keep_chasing = self.state == State::Chase && dist < aggro * LEASH;
            if input.player_alive && (dist < aggro || keep_chasing) {
                self.state = State::Chase;
            } else if self.state == State::Chase {
                self.start_idle(rng);
            }
        }

        let (walk, run) = self.species.speeds();
        match self.state {
            State::Idle => {
                if self.state_timer <= 0.0 {
                    let offset = vec2(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0)) * 120.0;
                    self.wander_target = self.pos + offset;
                    self.state = State::Wander;
                    self.state_timer = rng.range(3.0, 6.0);
                }
            }
            State::Wander => {
                let to_target = self.wander_target - self.pos;
                let stuck = !self.step(world, to_target, walk, dt);
                if to_target.length() < 4.0 || self.state_timer <= 0.0 || stuck {
                    self.start_idle(rng);
                }
            }
            State::Flee => {
                self.step(world, -to_player, run, dt);
                if self.state_timer <= 0.0 {
                    self.start_idle(rng);
                }
            }
            State::Chase => {
                let Some((damage, cooldown, range, _)) = self.species.attack() else {
                    self.start_idle(rng);
                    return 0.0;
                };
                self.facing_left = to_player.x < 0.0;
                if dist - self.species.radius() <= range {
                    if self.attack_cooldown <= 0.0 {
                        self.attack_cooldown = cooldown;
                        self.lunge = 0.25;
                        return damage;
                    }
                } else {
                    self.step(world, to_player, run, dt);
                }
            }
        }
        0.0
    }

    fn start_idle(&mut self, rng: &mut Rng) {
        self.state = State::Idle;
        self.state_timer = rng.range(1.0, 4.0);
    }

    /// Move na direção `dir`, deslizando em obstáculos. Retorna false se não saiu do lugar.
    fn step(&mut self, world: &World, dir: Vec2, speed: f32, dt: f32) -> bool {
        let dir = dir.normalize_or_zero();
        if dir == Vec2::ZERO {
            return false;
        }
        if dir.x.abs() > 0.05 {
            self.facing_left = dir.x < 0.0;
        }
        let start = self.pos;
        let delta = dir * speed * dt;
        for axis in [vec2(delta.x, 0.0), vec2(0.0, delta.y)] {
            let next = self.pos + axis;
            if self.species.can_stand(world, next) {
                self.pos = next;
            }
        }
        self.moving = self.pos != start;
        if self.moving {
            self.anim_time += dt * speed / 60.0;
        }
        self.moving
    }
}

/// Tenta repor a população de cada espécie até o limite.
/// `attempts` controla quantos tiles aleatórios testar por espécie (alto no início do mundo).
pub fn populate(creatures: &mut Vec<Creature>, world: &World, player_pos: Vec2, rng: &mut Rng, attempts: usize) {
    for species in Species::ALL {
        let mut alive = creatures
            .iter()
            .filter(|c| c.species == species && c.is_alive())
            .count();
        for _ in 0..attempts {
            if alive >= species.population_cap() {
                break;
            }
            let x = rng.range_u32(1, world.width as u32 - 2) as i32;
            let y = rng.range_u32(1, world.height as u32 - 2) as i32;
            let pos = vec2((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE);
            if spawn_ok(species, world, x, y, pos, player_pos) {
                creatures.push(Creature::new(species, pos));
                alive += 1;
            }
        }
    }
}

fn spawn_ok(species: Species, world: &World, x: i32, y: i32, pos: Vec2, player_pos: Vec2) -> bool {
    // Checagens baratas primeiro; colisão (mais cara) por último.
    if !species.habitat(world, x, y) || pos.distance(player_pos) < 420.0 {
        return false;
    }
    if species.temperament() == Temperament::Aggressive {
        let near_spawn = pos.distance(world.spawn) < SAFE_ZONE;
        let near_base = world
            .structures
            .iter()
            .any(|s| s.rect().center().distance(pos) < BASE_SAFE_ZONE);
        if near_spawn || near_base {
            return false;
        }
    }
    species.can_stand(world, pos)
}

/// Estado salvo de uma criatura (a IA recomeça do zero ao carregar).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreatureSave {
    pub species: Species,
    pub pos: [f32; 2],
    pub health: f32,
    pub corpse: Option<Corpse>,
}

impl From<&Creature> for CreatureSave {
    fn from(c: &Creature) -> Self {
        Self {
            species: c.species,
            pos: c.pos.into(),
            health: c.health,
            corpse: c.corpse,
        }
    }
}

impl From<CreatureSave> for Creature {
    fn from(s: CreatureSave) -> Self {
        let mut c = Creature::new(s.species, Vec2::from(s.pos));
        c.health = s.health.min(s.species.max_health());
        c.corpse = s.corpse;
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        World::generate(1337, 160, 160)
    }

    fn input(player_pos: Vec2) -> AiInput {
        AiInput {
            player_pos,
            player_alive: true,
        }
    }

    /// Um ponto de grama longe do spawn, livre para uma criatura.
    fn open_grass(world: &World, species: Species) -> Vec2 {
        for y in 0..world.height as i32 {
            for x in 0..world.width as i32 {
                let pos = vec2((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE);
                let free_around = [
                    vec2(0.0, 0.0),
                    vec2(40.0, 0.0),
                    vec2(-40.0, 0.0),
                    vec2(0.0, 40.0),
                    vec2(0.0, -40.0),
                ]
                .iter()
                .all(|&o| species.can_stand(world, pos + o));
                if world.tile(x, y) == Tile::Grass && free_around {
                    return pos;
                }
            }
        }
        panic!("sem grama livre");
    }

    #[test]
    fn raptor_chases_and_bites_player() {
        let world = world();
        let mut rng = Rng::new(3);
        let pos = open_grass(&world, Species::Raptor);
        let mut raptor = Creature::new(Species::Raptor, pos);
        let player = pos + vec2(30.0, 0.0);
        let mut damage = 0.0;
        for _ in 0..120 {
            damage += raptor.update(&world, &input(player), &mut rng, 1.0 / 60.0);
        }
        assert_eq!(raptor.state, State::Chase);
        assert!(damage > 0.0, "deveria ter atacado");
    }

    #[test]
    fn dodo_flees_when_hit() {
        let world = world();
        let mut rng = Rng::new(4);
        let pos = open_grass(&world, Species::Dodo);
        let mut dodo = Creature::new(Species::Dodo, pos);
        let player = pos - vec2(20.0, 0.0);
        assert!(!dodo.take_hit(5.0, player));
        assert_eq!(dodo.state, State::Flee);
        let before = dodo.pos.distance(player);
        for _ in 0..30 {
            assert_eq!(dodo.update(&world, &input(player), &mut rng, 1.0 / 60.0), 0.0);
        }
        assert!(dodo.pos.distance(player) > before);
    }

    #[test]
    fn death_leaves_a_harvestable_corpse() {
        let mut dilo = Creature::new(Species::Dilophosaur, Vec2::ZERO);
        assert!(!dilo.take_hit(30.0, Vec2::ONE));
        assert!(dilo.take_hit(30.0, Vec2::ONE));
        assert!(!dilo.is_alive());
        assert!(!dilo.take_hit(30.0, Vec2::ONE), "não morre duas vezes");
        for _ in 0..Species::Dilophosaur.harvest_hits() {
            assert!(dilo.harvest());
        }
        assert!(!dilo.harvest());
        assert!(dilo.is_gone());
    }

    #[test]
    fn populate_respects_caps_and_safe_zones() {
        let world = world();
        let mut rng = Rng::new(5);
        let mut creatures = Vec::new();
        populate(&mut creatures, &world, world.spawn, &mut rng, 5000);
        for species in Species::ALL {
            let n = creatures.iter().filter(|c| c.species == species).count();
            assert!(n <= species.population_cap(), "{species:?}");
            assert!(n > 0, "{species:?} deveria existir");
        }
        for c in &creatures {
            assert!(c.pos.distance(world.spawn) >= 420.0);
            if c.species.temperament() == Temperament::Aggressive {
                assert!(c.pos.distance(world.spawn) >= SAFE_ZONE);
            }
        }
        let before = creatures.len();
        populate(&mut creatures, &world, world.spawn, &mut rng, 5000);
        assert_eq!(creatures.len(), before, "já está no limite");
    }

    #[test]
    fn save_round_trip() {
        let mut c = Creature::new(Species::Raptor, vec2(10.0, 20.0));
        c.take_hit(200.0, Vec2::ZERO);
        let restored = Creature::from(CreatureSave::from(&c));
        assert_eq!(CreatureSave::from(&restored), CreatureSave::from(&c));
    }
}
