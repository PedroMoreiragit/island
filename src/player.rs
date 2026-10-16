//! Personagem: movimento livre em 8 direções, mas o sprite só vira para esquerda/direita.

use macroquad::prelude::*;

use crate::world::World;

pub const RADIUS: f32 = 7.0;
pub const WALK_SPEED: f32 = 130.0;
/// Distância máxima (da borda do nó) para coletar.
pub const REACH: f32 = 30.0;
pub const SWING_TIME: f32 = 0.3;
pub const SWING_COOLDOWN: f32 = 0.45;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Facing {
    Left,
    Right,
}

impl Facing {
    pub fn sign(self) -> f32 {
        match self {
            Facing::Left => -1.0,
            Facing::Right => 1.0,
        }
    }
}

pub struct Player {
    pub pos: Vec2,
    pub facing: Facing,
    pub moving: bool,
    pub anim_time: f32,
    pub swing_timer: f32,
    pub cooldown: f32,
}

impl Player {
    pub fn new(pos: Vec2) -> Self {
        Self {
            pos,
            facing: Facing::Right,
            moving: false,
            anim_time: 0.0,
            swing_timer: 0.0,
            cooldown: 0.0,
        }
    }

    pub fn face_towards(&mut self, x: f32) {
        if (x - self.pos.x).abs() > 2.0 {
            self.facing = if x < self.pos.x { Facing::Left } else { Facing::Right };
        }
    }

    pub fn update(&mut self, input: Vec2, world: &World, dt: f32) {
        self.swing_timer = (self.swing_timer - dt).max(0.0);
        self.cooldown = (self.cooldown - dt).max(0.0);

        let dir = input.normalize_or_zero();
        self.moving = dir != Vec2::ZERO;
        if !self.moving {
            return;
        }
        if dir.x.abs() > 0.01 {
            self.facing = if dir.x < 0.0 { Facing::Left } else { Facing::Right };
        }
        self.anim_time += dt;

        // Eixos separados: o jogador desliza ao encostar em obstáculos.
        let delta = dir * WALK_SPEED * world.tile_at(self.pos).speed_factor() * dt;
        let next = self.pos + vec2(delta.x, 0.0);
        if !world.blocked(next, RADIUS) {
            self.pos = next;
        }
        let next = self.pos + vec2(0.0, delta.y);
        if !world.blocked(next, RADIUS) {
            self.pos = next;
        }
    }
}
