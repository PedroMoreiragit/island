//! Estado do jogo e loop de update/draw. Orquestra os módulos; regras ficam neles.

use macroquad::prelude::*;

use crate::crafting::{self, CraftError, RECIPES};
use crate::gathering;
use crate::inventory::{HOTBAR_SIZE, INVENTORY_SIZE, Inventory};
use crate::items::{ItemId, ToolKind};
use crate::player::{self, Player};
use crate::render::{self, POPUP_LIFETIME, Popup};
use crate::rng::Rng;
use crate::ui::{self, UiAction, UiState};
use crate::world::World;

const MAP_SIZE: usize = 160;
/// Pixels de tela por pixel de mundo.
const ZOOM: f32 = 2.0;
const MESSAGE_TTL: f32 = 3.0;

pub struct Game {
    world: World,
    player: Player,
    inventory: Inventory,
    hotbar_selected: usize,
    ui: UiState,
    popups: Vec<Popup>,
    messages: Vec<(String, f32)>,
    rng: Rng,
    camera_pos: Vec2,
    time: f32,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let world = World::generate(seed, MAP_SIZE, MAP_SIZE);
        let player = Player::new(world.spawn);
        Self {
            camera_pos: player.pos,
            world,
            player,
            inventory: Inventory::new(INVENTORY_SIZE),
            hotbar_selected: 0,
            ui: UiState::default(),
            popups: Vec::new(),
            messages: Vec::new(),
            rng: Rng::new(seed ^ 0xA11CE),
            time: 0.0,
        }
    }

    /// Itens iniciais para testes (F1 em builds de debug).
    pub fn give_debug_items(&mut self) {
        for (item, n) in [
            (ItemId::Wood, 30),
            (ItemId::Stone, 40),
            (ItemId::Thatch, 50),
            (ItemId::Flint, 10),
        ] {
            self.inventory.add(item, n);
        }
        self.notify("Itens de debug adicionados");
    }

    pub fn open_inventory(&mut self) {
        self.ui.inventory_open = true;
    }

    fn notify(&mut self, msg: impl Into<String>) {
        self.messages.push((msg.into(), MESSAGE_TTL));
    }

    fn equipped(&self) -> Option<ItemId> {
        self.inventory.slots[self.hotbar_selected].map(|s| s.item)
    }

    fn equipped_tool(&self) -> Option<ToolKind> {
        self.equipped().and_then(ItemId::tool)
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        if is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::I) {
            self.ui.inventory_open = !self.ui.inventory_open;
            if !self.ui.inventory_open {
                ui::return_held(&mut self.ui, &mut self.inventory);
            }
        }
        if is_key_pressed(KeyCode::Escape) && self.ui.inventory_open {
            self.ui.inventory_open = false;
            ui::return_held(&mut self.ui, &mut self.inventory);
        }
        #[cfg(debug_assertions)]
        if is_key_pressed(KeyCode::F1) {
            self.give_debug_items();
        }

        self.update_hotbar_keys();

        match ui::handle_input(&mut self.ui, &mut self.inventory) {
            Some(UiAction::Craft(i)) => self.craft(i),
            Some(UiAction::SelectHotbar(i)) => self.hotbar_selected = i,
            None => {}
        }

        let input = if self.ui.inventory_open {
            Vec2::ZERO
        } else {
            movement_input()
        };
        self.player.update(input, &self.world, dt);

        let wants_gather =
            is_key_down(KeyCode::E) || (is_mouse_button_down(MouseButton::Left) && !ui::pointer_over_ui(&self.ui));
        if !self.ui.inventory_open && wants_gather && self.player.cooldown <= 0.0 {
            self.swing();
        }

        self.world.update(dt, self.player.pos);

        for p in &mut self.popups {
            p.age += dt;
        }
        self.popups.retain(|p| p.age < POPUP_LIFETIME);
        for m in &mut self.messages {
            m.1 -= dt;
        }
        self.messages.retain(|m| m.1 > 0.0);

        let follow = 1.0 - (-10.0 * dt).exp();
        self.camera_pos = self.camera_pos.lerp(self.player.pos, follow);
    }

    fn update_hotbar_keys(&mut self) {
        let keys = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
        ];
        if let Some(i) = keys.iter().position(|&k| is_key_pressed(k)) {
            self.hotbar_selected = i;
        }
        if !self.ui.inventory_open {
            let wheel = mouse_wheel().1;
            if wheel != 0.0 {
                let step = if wheel < 0.0 { 1 } else { HOTBAR_SIZE - 1 };
                self.hotbar_selected = (self.hotbar_selected + step) % HOTBAR_SIZE;
            }
        }
    }

    /// Nó vivo mais próximo ao alcance, preferindo os que estão à frente do personagem.
    fn find_target(&self) -> Option<usize> {
        let p = &self.player;
        self.world
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_alive())
            .filter_map(|(i, n)| {
                let gap = n.pos.distance(p.pos) - n.kind.collision_radius();
                if gap > player::REACH {
                    return None;
                }
                let behind = (n.pos.x - p.pos.x) * p.facing.sign() < -4.0;
                Some((i, gap + if behind { 20.0 } else { 0.0 }))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn swing(&mut self) {
        self.player.cooldown = player::SWING_COOLDOWN;
        self.player.swing_timer = player::SWING_TIME;
        let Some(idx) = self.find_target() else { return };

        let tool = self.equipped_tool();
        let node = &mut self.world.nodes[idx];
        node.hp -= 1;
        node.shake = 0.2;
        let (kind, pos) = (node.kind, node.pos);
        if node.hp == 0 {
            node.respawn_timer = kind.respawn_secs();
        }
        self.player.face_towards(pos.x);

        let drops = gathering::roll_yield(kind, tool, &mut self.rng);
        let mut full = false;
        for (i, (item, n)) in drops.into_iter().enumerate() {
            let leftover = self.inventory.add(item, n);
            if leftover < n {
                self.popups.push(Popup {
                    pos: pos - vec2(0.0, 30.0 + i as f32 * 14.0),
                    text: format!("+{} {}", n - leftover, item.name()),
                    age: 0.0,
                });
            }
            full |= leftover > 0;
        }
        if full {
            self.notify("Inventário cheio!");
        }
    }

    fn craft(&mut self, index: usize) {
        let recipe = &RECIPES[index];
        match crafting::craft(&mut self.inventory, recipe) {
            Ok(()) => self.notify(format!("Fabricado: {}", recipe.output.name())),
            Err(CraftError::MissingIngredients) => self.notify("Faltam materiais"),
            Err(CraftError::InventoryFull) => self.notify("Inventário cheio!"),
        }
    }

    fn camera(&self) -> Camera2D {
        let half = vec2(screen_width(), screen_height()) / (2.0 * ZOOM);
        let world_size = self.world.size_px();
        let target = vec2(
            self.camera_pos.x.clamp(half.x, (world_size.x - half.x).max(half.x)),
            self.camera_pos.y.clamp(half.y, (world_size.y - half.y).max(half.y)),
        );
        // zoom.y positivo => Y cresce para baixo (sem render target no macroquad 0.4).
        Camera2D {
            target,
            zoom: vec2(2.0 * ZOOM / screen_width(), 2.0 * ZOOM / screen_height()),
            ..Default::default()
        }
    }

    pub fn draw(&self) {
        clear_background(render::DEEP_WATER);
        let camera = self.camera();
        set_camera(&camera);

        let half = vec2(screen_width(), screen_height()) / (2.0 * ZOOM);
        let view = Rect::new(
            camera.target.x - half.x,
            camera.target.y - half.y,
            half.x * 2.0,
            half.y * 2.0,
        );
        render::draw_ground(&self.world, view, self.time);
        let target = if self.ui.inventory_open {
            None
        } else {
            self.find_target()
        };
        render::draw_entities(&self.world, &self.player, self.equipped(), target, view);

        set_default_camera();
        render::draw_popups(&self.popups, &camera);
        ui::draw(&self.ui, &self.inventory, self.hotbar_selected, &self.messages);
    }
}

fn movement_input() -> Vec2 {
    let mut v = Vec2::ZERO;
    if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) {
        v.y -= 1.0;
    }
    if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) {
        v.y += 1.0;
    }
    if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) {
        v.x -= 1.0;
    }
    if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) {
        v.x += 1.0;
    }
    v
}
