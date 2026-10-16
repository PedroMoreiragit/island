//! Desenho do mundo em visão 3/4. Arte provisória feita com primitivas:
//! cada entidade tem uma função `draw_*` isolada para ser trocada por sprites depois.

use macroquad::prelude::*;

use crate::items::ItemId;
use crate::player::{Player, SWING_TIME};
use crate::rng::hash2;
use crate::world::{NodeKind, ResourceNode, TILE, Tile, World};

pub const DEEP_WATER: Color = Color::new(0.09, 0.31, 0.55, 1.0);
const SHALLOW_WATER: Color = Color::new(0.20, 0.55, 0.74, 1.0);
const SAND: Color = Color::new(0.89, 0.80, 0.59, 1.0);
const GRASS: Color = Color::new(0.38, 0.63, 0.28, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.22);

fn shade(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, c.a)
}

pub fn draw_ground(world: &World, view: Rect, time: f32) {
    let x0 = (view.x / TILE).floor().max(0.0) as i32;
    let y0 = (view.y / TILE).floor().max(0.0) as i32;
    let x1 = ((view.right() / TILE).ceil() as i32).min(world.width as i32);
    let y1 = ((view.bottom() / TILE).ceil() as i32).min(world.height as i32);

    for y in y0..y1 {
        for x in x0..x1 {
            let tile = world.tile(x, y);
            let (px, py) = (x as f32 * TILE, y as f32 * TILE);
            let h = hash2(x, y, world.seed);
            let variation = 0.95 + h * 0.08;
            let color = match tile {
                Tile::DeepWater => shade(DEEP_WATER, 0.97 + (time * 0.8 + h * 6.0).sin() * 0.03),
                Tile::ShallowWater => shade(
                    SHALLOW_WATER,
                    1.0 + (time * 1.5 + x as f32 * 0.7 + y as f32 * 0.5).sin() * 0.04,
                ),
                Tile::Sand => shade(SAND, variation),
                Tile::Grass => shade(GRASS, variation),
            };
            draw_rectangle(px, py, TILE + 0.5, TILE + 0.5, color);

            match tile {
                Tile::Grass if h > 0.6 => {
                    let c = shade(GRASS, 0.78);
                    let (gx, gy) = (px + 6.0 + h * 16.0, py + 10.0 + (h * 37.0) % 14.0);
                    draw_line(gx, gy, gx - 2.0, gy - 5.0, 1.5, c);
                    draw_line(gx + 3.0, gy, gx + 4.0, gy - 6.0, 1.5, c);
                }
                Tile::Sand if h > 0.75 => {
                    draw_circle(px + h * 28.0, py + (h * 53.0) % 28.0, 1.2, shade(SAND, 0.82));
                }
                Tile::ShallowWater => draw_foam(world, x, y, px, py, time),
                _ => {}
            }
        }
    }
}

/// Espuma nas bordas da água rasa que encostam em terra.
fn draw_foam(world: &World, x: i32, y: i32, px: f32, py: f32, time: f32) {
    let alpha = 0.35 + (time * 2.0 + (x + y) as f32 * 0.6).sin() * 0.15;
    let foam = Color::new(1.0, 1.0, 1.0, alpha);
    let w = 4.0;
    if world.tile(x, y - 1).is_land() {
        draw_rectangle(px, py, TILE, w, foam);
    }
    if world.tile(x, y + 1).is_land() {
        draw_rectangle(px, py + TILE - w, TILE, w, foam);
    }
    if world.tile(x - 1, y).is_land() {
        draw_rectangle(px, py, w, TILE, foam);
    }
    if world.tile(x + 1, y).is_land() {
        draw_rectangle(px + TILE - w, py, w, TILE, foam);
    }
}

enum Drawable<'a> {
    Node(&'a ResourceNode),
    Player,
}

/// Desenha nós e jogador ordenados por Y (quem está mais "embaixo" fica na frente).
pub fn draw_entities(world: &World, player: &Player, held: Option<ItemId>, target: Option<usize>, view: Rect) {
    if let Some(node) = target.map(|i| &world.nodes[i]) {
        let r = node.kind.collision_radius().max(8.0) + 6.0;
        draw_ellipse_lines(
            node.pos.x,
            node.pos.y,
            r,
            r * 0.45,
            0.0,
            1.5,
            Color::new(1.0, 1.0, 1.0, 0.7),
        );
    }

    let margin = view.w.max(view.h) * 0.1 + 64.0;
    let culled = Rect::new(
        view.x - margin,
        view.y - margin,
        view.w + margin * 2.0,
        view.h + margin * 2.0,
    );
    let mut list: Vec<(f32, Drawable)> = world
        .nodes
        .iter()
        .filter(|n| culled.contains(n.pos))
        .map(|n| (n.pos.y, Drawable::Node(n)))
        .collect();
    list.push((player.pos.y, Drawable::Player));
    list.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (_, d) in list {
        match d {
            Drawable::Node(n) => draw_node(n, player.pos),
            Drawable::Player => draw_player(player, held),
        }
    }
}

fn draw_node(node: &ResourceNode, player_pos: Vec2) {
    let shake = if node.shake > 0.0 {
        (node.shake * 60.0).sin() * 2.0
    } else {
        0.0
    };
    let pos = node.pos + vec2(shake, 0.0);
    match (node.kind, node.is_alive()) {
        (NodeKind::Tree, true) => {
            // Copa translúcida quando o jogador está atrás da árvore.
            let behind = player_pos.y < node.pos.y
                && player_pos.y > node.pos.y - 70.0
                && (player_pos.x - node.pos.x).abs() < 26.0;
            draw_tree(pos, node.variant, if behind { 0.45 } else { 1.0 });
        }
        (NodeKind::Tree, false) => {
            draw_ellipse(pos.x, pos.y, 9.0, 4.0, 0.0, SHADOW);
            draw_rectangle(pos.x - 5.0, pos.y - 6.0, 10.0, 6.0, Color::from_rgba(110, 72, 40, 255));
            draw_ellipse(pos.x, pos.y - 6.0, 5.0, 2.0, 0.0, Color::from_rgba(196, 158, 108, 255));
        }
        (NodeKind::Rock, true) => draw_rock(pos, node.variant),
        (NodeKind::Pebble, true) => draw_pebbles(pos, node.variant),
        _ => {}
    }
}

fn draw_tree(pos: Vec2, variant: u8, alpha: f32) {
    let (x, y) = (pos.x, pos.y);
    draw_ellipse(x, y, 16.0, 6.0, 0.0, SHADOW);
    draw_rectangle(x - 4.0, y - 26.0, 8.0, 26.0, Color::from_rgba(110, 72, 40, 255));
    draw_rectangle(x - 4.0, y - 26.0, 3.0, 26.0, Color::from_rgba(132, 90, 52, 255));

    let leaf = |r, g, b| Color::from_rgba(r, g, b, 255).with_alpha(alpha);
    if variant.is_multiple_of(2) {
        // Folhosa
        let dark = leaf(46, 104, 46);
        let mid = leaf(62, 130, 56);
        draw_circle(x - 12.0, y - 32.0, 14.0, dark);
        draw_circle(x + 12.0, y - 32.0, 14.0, dark);
        draw_circle(x, y - 42.0, 19.0, mid);
        draw_circle(x - 6.0, y - 48.0, 8.0, leaf(92, 160, 72));
    } else {
        // Pinheiro
        for (i, w) in [22.0, 17.0, 12.0].into_iter().enumerate() {
            let base = y - 18.0 - i as f32 * 13.0;
            let c = if i % 2 == 0 {
                leaf(40, 96, 56)
            } else {
                leaf(52, 116, 64)
            };
            draw_triangle(vec2(x - w, base), vec2(x + w, base), vec2(x, base - 24.0), c);
        }
    }
}

fn draw_rock(pos: Vec2, variant: u8) {
    let (x, y) = (pos.x, pos.y);
    let s = 1.0 + variant as f32 * 0.08;
    draw_ellipse(x, y, 20.0 * s, 7.0, 0.0, SHADOW);
    draw_ellipse(
        x,
        y - 8.0 * s,
        18.0 * s,
        11.0 * s,
        0.0,
        Color::from_rgba(104, 104, 112, 255),
    );
    draw_ellipse(
        x - 2.0,
        y - 12.0 * s,
        14.0 * s,
        8.0 * s,
        0.0,
        Color::from_rgba(140, 140, 148, 255),
    );
    draw_ellipse(
        x - 6.0,
        y - 15.0 * s,
        5.0,
        2.5,
        0.0,
        Color::from_rgba(180, 180, 186, 255),
    );
}

fn draw_pebbles(pos: Vec2, variant: u8) {
    let c = Color::from_rgba(130, 128, 132, 255);
    let hi = Color::from_rgba(170, 168, 172, 255);
    let offsets: &[(f32, f32, f32)] = match variant % 2 {
        0 => &[(-4.0, 0.0, 4.0), (3.0, -2.0, 3.0), (2.0, 3.0, 2.5)],
        _ => &[(0.0, 0.0, 4.5), (-5.0, 2.0, 2.5)],
    };
    for &(dx, dy, r) in offsets {
        draw_ellipse(pos.x + dx, pos.y + dy + 1.0, r, r * 0.5, 0.0, SHADOW);
        draw_ellipse(pos.x + dx, pos.y + dy - 1.0, r, r * 0.7, 0.0, c);
        draw_ellipse(pos.x + dx - 1.0, pos.y + dy - 2.0, r * 0.5, r * 0.3, 0.0, hi);
    }
}

fn draw_player(p: &Player, held: Option<ItemId>) {
    let s = p.facing.sign();
    let (x, y) = (p.pos.x, p.pos.y);
    let step = if p.moving { (p.anim_time * 12.0).sin() } else { 0.0 };
    let bob = step.abs() * -1.5;

    let skin = Color::from_rgba(232, 186, 146, 255);
    let shirt = Color::from_rgba(196, 82, 58, 255);
    let pants = Color::from_rgba(70, 64, 92, 255);
    let hair = Color::from_rgba(78, 50, 34, 255);

    draw_ellipse(x, y, 9.0, 3.5, 0.0, SHADOW);
    // Pernas
    draw_rectangle(x - 5.0 + step * 2.0, y - 9.0, 4.0, 9.0, pants);
    draw_rectangle(x + 1.0 - step * 2.0, y - 9.0, 4.0, 9.0, pants);
    // Tronco
    draw_rectangle(x - 6.0, y - 22.0 + bob, 12.0, 14.0, shirt);
    // Cabeça
    let hy = y - 28.0 + bob;
    draw_circle(x, hy, 7.0, skin);
    draw_rectangle(x - 7.0, hy - 7.0, 14.0, 4.0, hair);
    draw_rectangle(x - 7.0 * s, hy - 5.0, 4.0 * s, 6.0, hair);
    draw_rectangle(x + 3.0 * s - 1.0, hy - 1.0, 2.0, 3.0, Color::from_rgba(40, 30, 30, 255));

    // Braço + item: ângulo 0 = para frente; negativo = para cima.
    let angle = if p.swing_timer > 0.0 {
        let t = 1.0 - p.swing_timer / SWING_TIME;
        (-110.0 + 160.0 * (t * t * (3.0 - 2.0 * t))).to_radians()
    } else {
        60.0_f32.to_radians()
    };
    let dir = vec2(angle.cos() * s, angle.sin());
    let shoulder = vec2(x + 2.0 * s, y - 19.0 + bob);
    let hand = shoulder + dir * 8.0;
    draw_line(shoulder.x, shoulder.y, hand.x, hand.y, 3.0, skin);
    if let Some(item) = held {
        draw_held_item(item, hand, dir);
    }
}

fn draw_held_item(item: ItemId, hand: Vec2, dir: Vec2) {
    let wood = Color::from_rgba(150, 104, 60, 255);
    let stone = Color::from_rgba(150, 150, 158, 255);
    let perp = vec2(-dir.y, dir.x);
    match item {
        ItemId::StonePickaxe => {
            let tip = hand + dir * 14.0;
            draw_line(hand.x, hand.y, tip.x, tip.y, 2.0, wood);
            let (a, b) = (tip + perp * 6.0 - dir * 2.0, tip - perp * 6.0 - dir * 2.0);
            draw_line(a.x, a.y, b.x, b.y, 3.0, stone);
        }
        ItemId::StoneHatchet => {
            let tip = hand + dir * 13.0;
            draw_line(hand.x, hand.y, tip.x, tip.y, 2.0, wood);
            let head = tip - dir * 2.0 + perp * 3.0;
            draw_circle(head.x, head.y, 4.0, stone);
        }
        ItemId::WoodenSpear => {
            let (back, tip) = (hand - dir * 8.0, hand + dir * 20.0);
            draw_line(back.x, back.y, tip.x, tip.y, 2.0, wood);
            draw_triangle(
                tip + dir * 6.0,
                tip + perp * 2.5,
                tip - perp * 2.5,
                Color::from_rgba(60, 60, 70, 255),
            );
        }
        _ => {}
    }
}

/// Números flutuantes de coleta (“+3 Madeira”). Desenhados em espaço de tela para texto nítido.
pub struct Popup {
    pub pos: Vec2,
    pub text: String,
    pub age: f32,
}

pub const POPUP_LIFETIME: f32 = 1.2;

pub fn draw_popups(popups: &[Popup], camera: &Camera2D) {
    for p in popups {
        let t = p.age / POPUP_LIFETIME;
        let screen = camera.world_to_screen(p.pos - vec2(0.0, t * 24.0));
        let alpha = 1.0 - t * t;
        let size = 22.0;
        let w = measure_text(&p.text, None, size as u16, 1.0).width;
        draw_text(
            &p.text,
            screen.x - w / 2.0 + 1.0,
            screen.y + 1.0,
            size,
            Color::new(0.0, 0.0, 0.0, alpha * 0.6),
        );
        draw_text(
            &p.text,
            screen.x - w / 2.0,
            screen.y,
            size,
            Color::new(1.0, 0.95, 0.7, alpha),
        );
    }
}
