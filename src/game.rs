//! Estado do jogo e loop de update/draw. Orquestra os módulos; regras ficam neles.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::crafting::{self, CraftError, RECIPES};
use crate::gathering;
use crate::inventory::{HOTBAR_SIZE, INVENTORY_SIZE, Inventory, Stack};
use crate::items::{ItemId, ToolKind};
use crate::player::{self, Facing, Player};
use crate::render::{self, POPUP_LIFETIME, Popup};
use crate::rng::Rng;
use crate::save::{self, BagSave, SAVE_VERSION, SaveData};
use crate::ui::{self, UiAction, UiState};
use crate::vitals::{self, MAX_FOOD, MAX_WATER, Vitals};
use crate::world::{BagKind, GroundBag, Tile, World};

const MAP_SIZE: usize = 160;
/// Pixels de tela por pixel de mundo.
const ZOOM: f32 = 2.0;
const MESSAGE_TTL: f32 = 3.0;
const AUTOSAVE_SECS: f32 = 60.0;
const PICKUP_REACH: f32 = 28.0;

pub struct Game {
    world: World,
    player: Player,
    vitals: Vitals,
    dead: bool,
    inventory: Inventory,
    hotbar_selected: usize,
    ui: UiState,
    popups: Vec<Popup>,
    messages: Vec<(String, f32)>,
    rng: Rng,
    camera_pos: Vec2,
    time: f32,
    save_path: Option<PathBuf>,
    autosave_timer: f32,
}

impl Game {
    pub fn new(seed: u64) -> Self {
        let world = World::generate(seed, MAP_SIZE, MAP_SIZE);
        let player = Player::new(world.spawn);
        Self {
            camera_pos: player.pos,
            world,
            player,
            vitals: Vitals::full(),
            dead: false,
            inventory: Inventory::new(INVENTORY_SIZE),
            hotbar_selected: 0,
            ui: UiState::default(),
            popups: Vec::new(),
            messages: Vec::new(),
            rng: Rng::new(seed ^ 0xA11CE),
            time: 0.0,
            save_path: None,
            autosave_timer: AUTOSAVE_SECS,
        }
    }

    pub fn from_save(data: SaveData) -> Self {
        let mut game = Self::new(data.seed);
        game.time = data.time;
        game.player.pos = Vec2::from(data.player_pos);
        game.player.facing = if data.facing_left { Facing::Left } else { Facing::Right };
        game.camera_pos = game.player.pos;
        game.vitals = data.vitals;
        game.dead = data.vitals.is_dead();
        for (slot, saved) in game.inventory.slots.iter_mut().zip(data.inventory) {
            *slot = saved;
        }
        game.hotbar_selected = data.hotbar_selected.min(HOTBAR_SIZE - 1);
        for (i, hp, timer) in data.nodes {
            if let Some(node) = game.world.nodes.get_mut(i) {
                node.hp = hp.min(node.kind.max_hp());
                node.respawn_timer = timer;
            }
        }
        game.world.bags = data
            .bags
            .into_iter()
            .map(|b| GroundBag {
                pos: Vec2::from(b.pos),
                stacks: b.stacks,
                kind: b.kind,
                ttl: b.ttl,
            })
            .collect();
        game.notify("Jogo carregado");
        game
    }

    fn to_save(&self) -> SaveData {
        // Uma pilha presa no cursor também conta como inventário.
        let mut inventory = self.inventory.clone();
        if let Some(held) = self.ui.held {
            inventory.insert_stack(held);
        }
        SaveData {
            version: SAVE_VERSION,
            seed: self.world.seed,
            time: self.time,
            player_pos: self.player.pos.into(),
            facing_left: self.player.facing == Facing::Left,
            vitals: self.vitals,
            inventory: inventory.slots,
            hotbar_selected: self.hotbar_selected,
            nodes: self
                .world
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.hp != n.kind.max_hp())
                .map(|(i, n)| (i, n.hp, n.respawn_timer))
                .collect(),
            bags: self
                .world
                .bags
                .iter()
                .map(|b| BagSave {
                    pos: b.pos.into(),
                    stacks: b.stacks.clone(),
                    kind: b.kind,
                    ttl: b.ttl,
                })
                .collect(),
        }
    }

    /// Ativa save automático/manual nesse caminho.
    pub fn set_save_path(&mut self, path: PathBuf) {
        self.save_path = Some(path);
    }

    pub fn save(&mut self) {
        let Some(path) = self.save_path.clone() else { return };
        match save::write(&path, &self.to_save()) {
            Ok(()) => self.notify("Jogo salvo"),
            Err(e) => self.notify(format!("Falha ao salvar: {e}")),
        }
        self.autosave_timer = AUTOSAVE_SECS;
    }

    /// Itens iniciais para testes (F1 em builds de debug).
    pub fn give_debug_items(&mut self) {
        for (item, n) in [
            (ItemId::Wood, 30),
            (ItemId::Stone, 40),
            (ItemId::Thatch, 50),
            (ItemId::Flint, 10),
            (ItemId::Berry, 20),
        ] {
            self.inventory.add(item, n);
        }
        self.notify("Itens de debug adicionados");
    }

    pub fn open_inventory(&mut self) {
        self.ui.inventory_open = true;
    }

    pub fn kill(&mut self) {
        self.vitals.damage(f32::MAX);
        self.die();
    }

    /// Mensagens iguais seguidas não se acumulam: só renovam o tempo.
    fn notify(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        match self.messages.last_mut() {
            Some(last) if last.0 == msg => last.1 = MESSAGE_TTL,
            _ => self.messages.push((msg, MESSAGE_TTL)),
        }
    }

    fn popup(&mut self, pos: Vec2, text: String) {
        self.popups.push(Popup { pos, text, age: 0.0 });
    }

    fn equipped(&self) -> Option<ItemId> {
        self.inventory.slots[self.hotbar_selected].map(|s| s.item)
    }

    fn equipped_tool(&self) -> Option<ToolKind> {
        self.equipped().and_then(ItemId::tool)
    }

    fn set_inventory_open(&mut self, open: bool) {
        self.ui.inventory_open = open;
        if !open {
            ui::return_held(&mut self.ui, &mut self.inventory);
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.time += dt;

        if self.dead {
            if is_key_pressed(KeyCode::Enter) || is_mouse_button_pressed(MouseButton::Left) {
                self.respawn();
            }
        } else {
            self.update_alive(dt);
        }

        if is_key_pressed(KeyCode::F5) {
            self.save();
        }
        self.autosave_timer -= dt;
        if self.autosave_timer <= 0.0 {
            self.save();
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

    fn update_alive(&mut self, dt: f32) {
        if is_key_pressed(KeyCode::Tab) || is_key_pressed(KeyCode::I) {
            self.set_inventory_open(!self.ui.inventory_open);
        }
        if is_key_pressed(KeyCode::Escape) && self.ui.inventory_open {
            self.set_inventory_open(false);
        }
        #[cfg(debug_assertions)]
        {
            if is_key_pressed(KeyCode::F1) {
                self.give_debug_items();
            }
            if is_key_pressed(KeyCode::F2) {
                self.vitals.damage(25.0);
            }
        }

        self.update_hotbar_keys();

        match ui::handle_input(&mut self.ui, &mut self.inventory) {
            Some(UiAction::Craft(i)) => self.craft(i),
            Some(UiAction::SelectHotbar(i)) => self.hotbar_selected = i,
            Some(UiAction::Drop(stack)) => self.drop_stack(stack),
            None => {}
        }

        let free = !self.ui.inventory_open;
        let input = if free { movement_input() } else { Vec2::ZERO };
        let sprinting = free && input != Vec2::ZERO && is_key_down(KeyCode::LeftShift) && self.vitals.can_sprint();
        let mult = if sprinting { player::SPRINT_MULT } else { 1.0 };
        self.player.update(input, mult, &self.world, dt);
        self.vitals.tick(dt, sprinting && self.player.moving);

        if free {
            let over_ui = ui::pointer_over_ui(&self.ui);
            let wants_gather = is_key_down(KeyCode::E) || (is_mouse_button_down(MouseButton::Left) && !over_ui);
            if wants_gather && self.player.cooldown <= 0.0 {
                self.swing();
            }
            if is_key_pressed(KeyCode::R) || (is_mouse_button_pressed(MouseButton::Right) && !over_ui) {
                self.use_selected();
            }
            if is_key_pressed(KeyCode::Q) {
                let all = is_key_down(KeyCode::LeftControl);
                let count = if all { u32::MAX } else { 1 };
                if let Some(stack) = self.inventory.take_from_slot(self.hotbar_selected, count) {
                    self.drop_stack(stack);
                }
            }
            if is_key_pressed(KeyCode::F) {
                self.pick_up();
            }
        }

        if self.vitals.is_dead() {
            self.die();
        }
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

    /// Água ao alcance: no tile do jogador, à frente dele ou logo acima/abaixo.
    fn near_water(&self) -> bool {
        let p = self.player.pos;
        let s = self.player.facing.sign();
        [p, p + vec2(16.0 * s, 0.0), p + vec2(0.0, 16.0), p + vec2(0.0, -16.0)]
            .iter()
            .any(|&q| matches!(self.world.tile_at(q), Tile::ShallowWater | Tile::DeepWater))
    }

    fn swing(&mut self) {
        self.player.cooldown = player::SWING_COOLDOWN;
        let target = self.find_target();
        if target.is_none() && self.near_water() {
            self.drink();
            return;
        }
        if !self.vitals.spend_stamina(vitals::SWING_STAMINA) {
            self.notify("Sem stamina!");
            return;
        }
        self.player.swing_timer = player::SWING_TIME;
        let Some(idx) = target else { return };

        let tool = self.equipped_tool();
        let node = &mut self.world.nodes[idx];
        node.hp -= 1;
        node.shake = 0.2;
        let (kind, pos) = (node.kind, node.pos);
        if node.hp == 0 {
            node.respawn_timer = kind.respawn_secs();
        }
        self.player.face_towards(pos.x);

        // Só ferramentas de coleta desgastam ao coletar.
        if matches!(tool, Some(ToolKind::Pickaxe | ToolKind::Hatchet))
            && let Some(broken) = self.inventory.wear(self.hotbar_selected)
        {
            self.notify(format!("{} quebrou!", broken.name()));
        }

        let drops = gathering::roll_yield(kind, tool, &mut self.rng);
        let mut full = false;
        for (i, (item, n)) in drops.into_iter().enumerate() {
            let leftover = self.inventory.add(item, n);
            if leftover < n {
                let at = pos - vec2(0.0, 30.0 + i as f32 * 14.0);
                self.popup(at, format!("+{} {}", n - leftover, item.name()));
            }
            full |= leftover > 0;
        }
        if full {
            self.notify("Inventário cheio!");
        }
    }

    fn drink(&mut self) {
        if self.vitals.water >= MAX_WATER - 0.5 {
            self.notify("Você não está com sede");
            return;
        }
        self.vitals.drink(vitals::DRINK_WATER);
        let at = self.player.pos - vec2(0.0, 40.0);
        self.popup(at, format!("+{} Água", vitals::DRINK_WATER));
    }

    /// Usa o item da hotbar selecionado (por enquanto: comer).
    fn use_selected(&mut self) {
        let Some(item) = self.equipped() else { return };
        let Some(effect) = item.food_effect() else { return };
        if self.vitals.food >= MAX_FOOD - 0.5 {
            self.notify("Você está satisfeito");
            return;
        }
        self.inventory.take_from_slot(self.hotbar_selected, 1);
        self.vitals.eat(effect);
        let at = self.player.pos - vec2(0.0, 40.0);
        self.popup(at, format!("+{} Comida", effect.food));
    }

    fn drop_stack(&mut self, stack: Stack) {
        let ahead = self.player.pos + vec2(16.0 * self.player.facing.sign(), 4.0);
        let pos = if self.world.tile_at(ahead).walkable() {
            ahead
        } else {
            self.player.pos
        };
        self.world.drop_bag(pos, vec![stack], BagKind::Dropped);
    }

    fn nearest_bag(&self) -> Option<usize> {
        self.world.nearest_bag(self.player.pos, PICKUP_REACH)
    }

    fn pick_up(&mut self) {
        let Some(i) = self.nearest_bag() else { return };
        let bag = &mut self.world.bags[i];
        let before: u32 = bag.stacks.iter().map(|s| s.count).sum();
        self.inventory.take_all(&mut bag.stacks);
        let after: u32 = bag.stacks.iter().map(|s| s.count).sum();
        let pos = bag.pos;
        if after < before {
            self.popup(pos - vec2(0.0, 20.0), format!("+{} itens", before - after));
        }
        if after > 0 {
            self.notify("Inventário cheio!");
        }
    }

    fn die(&mut self) {
        if self.dead {
            return;
        }
        self.dead = true;
        self.set_inventory_open(false);
        let stacks = self.inventory.drain_all();
        self.world.drop_bag(self.player.pos, stacks, BagKind::Death);
        self.player.moving = false;
    }

    fn respawn(&mut self) {
        self.player = Player::new(self.world.spawn);
        self.camera_pos = self.player.pos;
        self.vitals = Vitals::full();
        self.dead = false;
        self.notify("Você renasceu na praia. Recupere sua bolsa!");
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
        let interactive = !self.ui.inventory_open && !self.dead;
        let target = if interactive { self.find_target() } else { None };
        let player = (!self.dead).then_some(&self.player);
        render::draw_entities(&self.world, player, self.equipped(), target, view, self.time);

        set_default_camera();
        render::draw_popups(&self.popups, &camera);
        if interactive && let Some(bag) = self.nearest_bag().map(|i| &self.world.bags[i]) {
            render::draw_bag_prompt(bag, &camera);
        }
        ui::draw(&ui::Hud {
            ui: &self.ui,
            inventory: &self.inventory,
            selected: self.hotbar_selected,
            messages: &self.messages,
            vitals: &self.vitals,
            dead: self.dead,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_round_trip_restores_state() {
        let mut game = Game::new(99);
        game.inventory.add(ItemId::StonePickaxe, 1);
        game.inventory.wear(0);
        game.inventory.add(ItemId::Berry, 7);
        game.hotbar_selected = 1;
        game.vitals.food = 42.0;
        game.player.pos += vec2(10.0, -5.0);
        game.world.nodes[5].hp = 0;
        game.world.nodes[5].respawn_timer = 33.0;
        game.world
            .drop_bag(vec2(50.0, 60.0), vec![Stack::new(ItemId::Wood, 3)], BagKind::Dropped);

        let saved = game.to_save();
        let json = serde_json::to_string(&saved).unwrap();
        let loaded = Game::from_save(serde_json::from_str(&json).unwrap());
        assert_eq!(loaded.to_save(), saved);
        assert_eq!(loaded.inventory.slots[0].unwrap().durability, 99);
    }

    #[test]
    fn death_drops_everything_in_a_bag() {
        let mut game = Game::new(1);
        game.inventory.add(ItemId::Stone, 10);
        game.kill();
        assert!(game.dead);
        assert_eq!(game.inventory.count(ItemId::Stone), 0);
        let bag = game.world.bags.iter().find(|b| b.kind == BagKind::Death).unwrap();
        assert_eq!(bag.stacks[0].count, 10);
        game.respawn();
        assert!(!game.dead && !game.vitals.is_dead());
    }
}
