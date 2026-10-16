//! Mapa da ilha (tiles) e nós de recurso (árvores, rochas, pedrinhas).
//! Coordenadas de mundo em pixels; `pos` de entidades é o ponto do "pé" (base), usado no y-sort.

use macroquad::prelude::*;

use crate::rng::{Rng, fbm};

pub const TILE: f32 = 32.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tile {
    DeepWater,
    ShallowWater,
    Sand,
    Grass,
}

impl Tile {
    pub fn walkable(self) -> bool {
        self != Tile::DeepWater
    }

    pub fn is_land(self) -> bool {
        matches!(self, Tile::Sand | Tile::Grass)
    }

    pub fn speed_factor(self) -> f32 {
        match self {
            Tile::ShallowWater => 0.55,
            Tile::Sand => 0.9,
            _ => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Tree,
    Rock,
    Pebble,
}

impl NodeKind {
    /// Golpes até esgotar.
    pub fn max_hp(self) -> u32 {
        match self {
            NodeKind::Tree => 8,
            NodeKind::Rock => 10,
            NodeKind::Pebble => 1,
        }
    }

    pub fn respawn_secs(self) -> f32 {
        match self {
            NodeKind::Tree => 90.0,
            NodeKind::Rock => 120.0,
            NodeKind::Pebble => 45.0,
        }
    }

    /// Raio de colisão com o jogador (0 = atravessável).
    pub fn collision_radius(self) -> f32 {
        match self {
            NodeKind::Tree => 9.0,
            NodeKind::Rock => 16.0,
            NodeKind::Pebble => 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceNode {
    pub kind: NodeKind,
    pub pos: Vec2,
    pub hp: u32,
    pub respawn_timer: f32,
    pub variant: u8,
    /// Tempo restante do "tremor" visual após um golpe.
    pub shake: f32,
}

impl ResourceNode {
    pub fn is_alive(&self) -> bool {
        self.hp > 0
    }
}

pub struct World {
    pub width: usize,
    pub height: usize,
    pub seed: u64,
    tiles: Vec<Tile>,
    pub nodes: Vec<ResourceNode>,
    pub spawn: Vec2,
}

impl World {
    pub fn generate(seed: u64, width: usize, height: usize) -> Self {
        let mut tiles = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let nx = x as f32 / width as f32 * 2.0 - 1.0;
                let ny = y as f32 / height as f32 * 2.0 - 1.0;
                let dist = (nx * nx + ny * ny).sqrt();
                let noise = fbm(x as f32 / 22.0, y as f32 / 22.0, seed, 4);
                let h = 1.0 - dist * 1.15 + (noise - 0.5) * 0.5;
                let border = x == 0 || y == 0 || x == width - 1 || y == height - 1;
                tiles.push(match h {
                    _ if border => Tile::DeepWater,
                    h if h < 0.18 => Tile::DeepWater,
                    h if h < 0.27 => Tile::ShallowWater,
                    h if h < 0.36 => Tile::Sand,
                    _ => Tile::Grass,
                });
            }
        }

        let mut world = Self {
            width,
            height,
            seed,
            tiles,
            nodes: Vec::new(),
            spawn: Vec2::ZERO,
        };
        world.spawn = world.find_beach_spawn();
        world.scatter_nodes();
        world
    }

    /// Primeiro tile de areia subindo a partir do centro da borda sul.
    fn find_beach_spawn(&self) -> Vec2 {
        let cx = self.width / 2;
        for y in (0..self.height).rev() {
            for dx in 0..self.width / 4 {
                for x in [cx + dx, cx - dx] {
                    if self.tile(x as i32, y as i32) == Tile::Sand {
                        return vec2((x as f32 + 0.5) * TILE, (y as f32 - 1.5) * TILE);
                    }
                }
            }
        }
        vec2(self.width as f32, self.height as f32) * TILE * 0.5
    }

    fn scatter_nodes(&mut self) {
        let mut rng = Rng::new(self.seed.wrapping_add(1));
        let forest_seed = self.seed.wrapping_add(7);
        for y in 0..self.height as i32 {
            for x in 0..self.width as i32 {
                let center = vec2((x as f32 + 0.5) * TILE, (y as f32 + 0.5) * TILE);
                if center.distance(self.spawn) < TILE * 3.0 {
                    continue;
                }
                let forest = fbm(x as f32 / 12.0, y as f32 / 12.0, forest_seed, 3);
                let kind = match self.tile(x, y) {
                    Tile::Grass => {
                        let tree_p = if forest > 0.55 { 0.22 } else { 0.03 };
                        if rng.chance(tree_p) {
                            Some(NodeKind::Tree)
                        } else if rng.chance(0.012) {
                            Some(NodeKind::Rock)
                        } else if rng.chance(0.015) {
                            Some(NodeKind::Pebble)
                        } else {
                            None
                        }
                    }
                    Tile::Sand if rng.chance(0.03) => Some(NodeKind::Pebble),
                    Tile::Sand if rng.chance(0.004) => Some(NodeKind::Rock),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let jitter = vec2(rng.range(-8.0, 8.0), rng.range(-8.0, 8.0));
                    self.nodes.push(ResourceNode {
                        kind,
                        pos: center + jitter,
                        hp: kind.max_hp(),
                        respawn_timer: 0.0,
                        variant: (rng.next_u64() % 4) as u8,
                        shake: 0.0,
                    });
                }
            }
        }
    }

    pub fn tile(&self, x: i32, y: i32) -> Tile {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return Tile::DeepWater;
        }
        self.tiles[y as usize * self.width + x as usize]
    }

    pub fn tile_at(&self, pos: Vec2) -> Tile {
        self.tile((pos.x / TILE).floor() as i32, (pos.y / TILE).floor() as i32)
    }

    pub fn size_px(&self) -> Vec2 {
        vec2(self.width as f32, self.height as f32) * TILE
    }

    /// Um círculo de raio `radius` centrado em `pos` colide com água funda ou com algum nó sólido?
    pub fn blocked(&self, pos: Vec2, radius: f32) -> bool {
        let probes = [pos, pos + vec2(-radius, 0.0), pos + vec2(radius, 0.0)];
        if probes.iter().any(|&p| !self.tile_at(p).walkable()) {
            return true;
        }
        self.nodes.iter().any(|n| {
            let r = n.kind.collision_radius();
            r > 0.0 && n.is_alive() && n.pos.distance_squared(pos) < (r + radius).powi(2)
        })
    }

    /// Avança respawns; um nó não renasce em cima do jogador.
    pub fn update(&mut self, dt: f32, player_pos: Vec2) {
        for node in &mut self.nodes {
            node.shake = (node.shake - dt).max(0.0);
            if node.is_alive() {
                continue;
            }
            node.respawn_timer -= dt;
            let clear = node.pos.distance(player_pos) > node.kind.collision_radius() + 16.0;
            if node.respawn_timer <= 0.0 && clear {
                node.hp = node.kind.max_hp();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_has_beach_and_valid_spawn() {
        let world = World::generate(1337, 160, 160);
        assert_eq!(world.tile_at(world.spawn), Tile::Sand);
        assert!(!world.blocked(world.spawn, 7.0));
        let count = |kind| world.nodes.iter().filter(|n| n.kind == kind).count();
        assert!(count(NodeKind::Tree) > 50);
        assert!(count(NodeKind::Rock) > 5);
        assert!(count(NodeKind::Pebble) > 20);
    }
}
