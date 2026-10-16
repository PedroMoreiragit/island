//! Estado do jogo e loop de update/draw. Orquestra os módulos; regras ficam neles.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::building::{self, Layer, PlaceError, StructureKind};
use crate::crafting::{CraftError, CraftQueue, RECIPES, Recipe};
use crate::daynight;
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
/// Luz fraca em volta do jogador para não ficar cego à noite.
const PLAYER_LIGHT: f32 = 56.0;

/// Algo com que o jogador pode interagir com F.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interaction {
    Bag(usize),
    Structure(usize),
}

pub struct Game {
    world: World,
    player: Player,
    vitals: Vitals,
    dead: bool,
    inventory: Inventory,
    hotbar_selected: usize,
    queue: CraftQueue,
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
            queue: CraftQueue::default(),
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
        game.queue = data.craft_queue;
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
        game.world.structures = data.structures;
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
            structures: self.world.structures.clone(),
            craft_queue: self.queue.clone(),
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

    // ---------- Atalhos de debug / smoke test ----------

    /// Itens iniciais para testes (F1 em builds de debug).
    pub fn give_debug_items(&mut self) {
        for (item, n) in [
            (ItemId::Wood, 100),
            (ItemId::Stone, 60),
            (ItemId::Thatch, 80),
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

    /// Avança o relógio até a próxima ocorrência de `hour`.
    pub fn skip_to_hour(&mut self, hour: f32) {
        self.time += daynight::secs_until(self.time, hour);
    }

    /// Monta uma cabana de exemplo ao norte do spawn (para screenshots).
    pub fn build_demo_base(&mut self) {
        let (sx, sy) = building::tile_of(self.world.spawn);
        let (x0, x1, y0, y1) = (sx - 2, sx + 2, sy - 5, sy - 2);
        let mut place = |kind: StructureKind, tile: (i32, i32)| {
            if self.world.tile(tile.0, tile.1).is_land() && self.world.structure_at(tile, kind.layer()).is_none() {
                self.world.place(kind, tile);
            }
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                place(StructureKind::WoodFoundation, (x, y));
                let edge = x == x0 || x == x1 || y == y0 || y == y1;
                if edge {
                    let door = y == y1 && x == sx;
                    place(
                        if door {
                            StructureKind::WoodDoor
                        } else {
                            StructureKind::WoodWall
                        },
                        (x, y),
                    );
                }
            }
        }
        place(StructureKind::StorageBox, (sx - 1, y0 + 1));
        place(StructureKind::Workbench, (sx + 1, y0 + 1));
        place(StructureKind::Campfire, (sx + 2, sy));
        if let Some(i) = self.world.structure_at((sx + 2, sy), Layer::Object) {
            self.world.structures[i].fuel = 300.0;
        }
    }

    // ---------- Helpers ----------

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

    /// Estrutura que o jogador está segurando para posicionar.
    fn placing_kind(&self) -> Option<StructureKind> {
        self.equipped().and_then(ItemId::structure)
    }

    fn station_ok(&self, recipe: &Recipe) -> bool {
        recipe
            .station
            .is_none_or(|kind| self.world.station_near(kind, self.player.pos))
    }

    fn chest_mut(&mut self) -> Option<&mut Inventory> {
        let i = self.ui.chest?;
        self.world.structures.get_mut(i)?.storage.as_mut()
    }

    fn set_inventory_open(&mut self, open: bool) {
        self.ui.inventory_open = open;
        if open {
            return;
        }
        // O que não couber de volta no inventário vai para o baú aberto ou para o chão.
        if let Some(left) = ui::return_held(&mut self.ui, &mut self.inventory) {
            let left = match self.chest_mut() {
                Some(chest) => chest.insert_stack(left),
                None => Some(left),
            };
            if let Some(stack) = left {
                self.drop_stack(stack);
            }
        }
        self.ui.chest = None;
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

    // ---------- Update ----------

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

        self.deliver_crafts(dt);
        self.world.update(dt, self.player.pos);
        self.validate_open_chest();

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
        if is_key_pressed(KeyCode::H) {
            self.ui.hide_help = !self.ui.hide_help;
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
            if is_key_pressed(KeyCode::F3) {
                self.time += daynight::DAY_LENGTH / 8.0;
                self.notify("+3 horas");
            }
        }

        self.update_hotbar_keys();

        let chest = self
            .ui
            .chest
            .and_then(|i| self.world.structures.get_mut(i))
            .and_then(|s| s.storage.as_mut());
        match ui::handle_input(&mut self.ui, &mut self.inventory, chest, &self.queue) {
            Some(UiAction::Craft(i)) => self.craft(i),
            Some(UiAction::CancelCraft(i)) => {
                for stack in self.queue.cancel(i, &mut self.inventory) {
                    self.drop_stack(stack);
                }
            }
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
            let placing = self.placing_kind().is_some();
            let mouse_gather = is_mouse_button_down(MouseButton::Left) && !over_ui && !placing;
            if (is_key_down(KeyCode::E) || mouse_gather) && self.player.cooldown <= 0.0 {
                self.swing();
            }
            if placing && is_mouse_button_pressed(MouseButton::Left) && !over_ui {
                let tile = self.mouse_tile();
                if let Err(e) = self.place_selected_at(tile) {
                    self.notify(e.message());
                }
            }
            if is_key_pressed(KeyCode::X) {
                let tile = self.mouse_tile();
                self.demolish_at(tile);
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
            if is_key_pressed(KeyCode::F)
                && let Some(target) = self.nearest_interaction()
            {
                self.interact(target);
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

    /// Entrega o que a fila de crafting terminou; se não couber, cai no chão.
    fn deliver_crafts(&mut self, dt: f32) {
        for stack in self.queue.tick(dt) {
            let name = stack.item.name();
            match self.inventory.insert_stack(stack) {
                None => self.notify(format!("Fabricado: {name}")),
                Some(left) => {
                    self.drop_stack(left);
                    self.notify(format!("Inventário cheio: {name} caiu no chão"));
                }
            }
        }
    }

    /// Fecha o baú se ele sumiu ou se o jogador se afastou.
    fn validate_open_chest(&mut self) {
        let Some(i) = self.ui.chest else { return };
        let still_valid = self.world.structures.get(i).is_some_and(|s| {
            s.kind == StructureKind::StorageBox
                && building::distance_to_rect(self.player.pos, s.rect()) <= building::INTERACT_REACH + 16.0
        });
        if !still_valid || self.dead {
            self.set_inventory_open(false);
        }
    }

    // ---------- Coleta, comida, água ----------

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

    // ---------- Construção ----------

    fn mouse_tile(&self) -> (i32, i32) {
        let world_pos = self.camera().screen_to_world(mouse_position().into());
        building::tile_of(world_pos)
    }

    fn place_selected_at(&mut self, tile: (i32, i32)) -> Result<(), PlaceError> {
        let Some(kind) = self.placing_kind() else {
            return Ok(());
        };
        self.world.can_place(kind, tile, self.player.pos, player::RADIUS)?;
        self.world.place(kind, tile);
        self.inventory.take_from_slot(self.hotbar_selected, 1);
        Ok(())
    }

    /// Remove a estrutura no tile (objeto antes do piso) e devolve o item; conteúdo de baú cai no chão.
    fn demolish_at(&mut self, tile: (i32, i32)) -> bool {
        if building::tile_rect(tile).center().distance(self.player.pos) > building::PLACE_REACH {
            return false;
        }
        let Some(i) = self
            .world
            .structure_at(tile, Layer::Object)
            .or_else(|| self.world.structure_at(tile, Layer::Floor))
        else {
            return false;
        };
        self.set_inventory_open(false);
        let mut s = self.world.structures.remove(i);
        let center = s.rect().center();
        if let Some(storage) = s.storage.take() {
            let stacks: Vec<Stack> = storage.slots.into_iter().flatten().collect();
            self.world.drop_bag(center, stacks, BagKind::Dropped);
        }
        let item = s.kind.item();
        if let Some(left) = self.inventory.insert_stack(Stack::new(item, 1)) {
            self.world.drop_bag(center, vec![left], BagKind::Dropped);
        }
        self.notify(format!("{} desmontada", item.name()));
        true
    }

    // ---------- Interação (F) ----------

    fn nearest_interaction(&self) -> Option<Interaction> {
        let p = self.player.pos;
        let bag = self
            .world
            .nearest_bag(p, PICKUP_REACH)
            .map(|i| (self.world.bags[i].pos.distance(p), Interaction::Bag(i)));
        let structure = self
            .world
            .structures
            .iter()
            .enumerate()
            .filter(|(_, s)| s.kind.interactive())
            .map(|(i, s)| (building::distance_to_rect(p, s.rect()), Interaction::Structure(i)))
            .filter(|(d, _)| *d <= building::INTERACT_REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        [bag, structure]
            .into_iter()
            .flatten()
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, target)| target)
    }

    fn interact(&mut self, target: Interaction) {
        match target {
            Interaction::Bag(i) => self.pick_up(i),
            Interaction::Structure(i) => match self.world.structures[i].kind {
                StructureKind::WoodDoor => self.toggle_door(i),
                StructureKind::StorageBox => {
                    self.ui.chest = Some(i);
                    self.ui.inventory_open = true;
                }
                StructureKind::Workbench => self.set_inventory_open(true),
                StructureKind::Campfire => self.feed_campfire(i),
                StructureKind::WoodFoundation | StructureKind::WoodWall => {}
            },
        }
    }

    fn toggle_door(&mut self, i: usize) {
        let door = &self.world.structures[i];
        let mut closed = door.clone();
        closed.open = false;
        if door.open && closed.blocks(self.player.pos, player::RADIUS) {
            self.notify("Saia da porta para fechá-la");
            return;
        }
        let door = &mut self.world.structures[i];
        door.open = !door.open;
    }

    fn feed_campfire(&mut self, i: usize) {
        if self.inventory.count(ItemId::Wood) == 0 {
            self.notify("Precisa de Madeira");
            return;
        }
        let fire = &mut self.world.structures[i];
        if !fire.add_fuel() {
            self.notify("A fogueira já está cheia");
            return;
        }
        let at = fire.rect().center() - vec2(0.0, 20.0);
        self.inventory.remove(ItemId::Wood, 1);
        self.popup(at, "+1 min de fogo".to_owned());
    }

    fn pick_up(&mut self, i: usize) {
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

    // ---------- Morte ----------

    fn die(&mut self) {
        if self.dead {
            return;
        }
        self.dead = true;
        self.set_inventory_open(false);
        // A fila é cancelada: os materiais voltam e vão junto para a bolsa.
        let mut leftovers = Vec::new();
        while !self.queue.jobs.is_empty() {
            leftovers.extend(self.queue.cancel(0, &mut self.inventory));
        }
        let mut stacks = self.inventory.drain_all();
        stacks.extend(leftovers);
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
        let station_near = self.station_ok(recipe);
        match self.queue.enqueue(&mut self.inventory, recipe, station_near) {
            Ok(()) => {}
            Err(CraftError::MissingIngredients) => self.notify("Faltam materiais"),
            Err(CraftError::NeedsStation(kind)) => self.notify(format!("Requer {} por perto", kind.item().name())),
            Err(CraftError::QueueFull) => self.notify("Fila de crafting cheia"),
        }
    }

    // ---------- Desenho ----------

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

        let hour = daynight::hour(self.time);
        let flicker = 1.0 + (self.time * 9.0).sin() * 0.03 + (self.time * 23.0).sin() * 0.02;
        let mut lights: Vec<(Vec2, f32, bool)> = self.world.lights().map(|(p, r)| (p, r * flicker, true)).collect();
        if !self.dead {
            lights.push((self.player.pos - vec2(0.0, 12.0), PLAYER_LIGHT, false));
        }
        render::draw_golden_hour(view, daynight::golden_hour(hour));
        render::draw_lighting(view, daynight::darkness(hour), &lights);

        if interactive && let Some(kind) = self.placing_kind() {
            let tile = self.mouse_tile();
            let valid = self
                .world
                .can_place(kind, tile, self.player.pos, player::RADIUS)
                .is_ok();
            render::draw_ghost(kind, tile, valid);
        }

        set_default_camera();
        render::draw_popups(&self.popups, &camera);
        if interactive && let Some(target) = self.nearest_interaction() {
            let (label, pos) = match target {
                Interaction::Bag(i) => {
                    let bag = &self.world.bags[i];
                    (render::bag_label(bag), bag.pos - vec2(0.0, 26.0))
                }
                Interaction::Structure(i) => {
                    let s = &self.world.structures[i];
                    let r = s.rect();
                    (render::structure_label(s), vec2(r.center().x, r.y - 30.0))
                }
            };
            render::draw_prompt(&label, pos, &camera);
        }

        let chest = self
            .ui
            .chest
            .and_then(|i| self.world.structures.get(i))
            .and_then(|s| s.storage.as_ref());
        ui::draw(&ui::Hud {
            ui: &self.ui,
            inventory: &self.inventory,
            chest,
            queue: &self.queue,
            station_ok: &|recipe| self.station_ok(recipe),
            selected: self.hotbar_selected,
            messages: &self.messages,
            vitals: &self.vitals,
            dead: self.dead,
            time: self.time,
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
        game.inventory.add(ItemId::Wood, 50);
        game.inventory.add(ItemId::Thatch, 50);
        game.hotbar_selected = 1;
        game.vitals.food = 42.0;
        game.player.pos += vec2(10.0, -5.0);
        game.world.nodes[5].hp = 0;
        game.world.nodes[5].respawn_timer = 33.0;
        game.world
            .drop_bag(vec2(50.0, 60.0), vec![Stack::new(ItemId::Wood, 3)], BagKind::Dropped);
        game.world.place(StructureKind::StorageBox, (3, 4));
        game.world.structures[0].storage.as_mut().unwrap().add(ItemId::Flint, 9);
        let spear = crate::crafting::recipe_for(ItemId::WoodenSpear).unwrap();
        game.queue.enqueue(&mut game.inventory, spear, false).unwrap();
        game.queue.tick(1.0);

        let saved = game.to_save();
        let json = serde_json::to_string(&saved).unwrap();
        let loaded = Game::from_save(serde_json::from_str(&json).unwrap());
        assert_eq!(loaded.to_save(), saved);
        assert_eq!(loaded.inventory.slots[0].unwrap().durability, 99);
        assert_eq!(loaded.queue.jobs.len(), 1);
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

    #[test]
    fn place_and_demolish_returns_item_and_chest_contents() {
        let mut game = Game::new(1337);
        game.inventory.add(ItemId::StorageBox, 1);
        let here = building::tile_of(game.player.pos);
        let next = (here.0 + 1, here.1);

        assert_eq!(game.place_selected_at(here), Err(PlaceError::Blocked));
        assert_eq!(game.place_selected_at(next), Ok(()));
        assert_eq!(game.inventory.count(ItemId::StorageBox), 0);

        let i = game.world.structure_at(next, Layer::Object).unwrap();
        game.world.structures[i].storage.as_mut().unwrap().add(ItemId::Stone, 5);
        assert!(game.demolish_at(next));
        assert_eq!(game.inventory.count(ItemId::StorageBox), 1);
        let dropped: u32 = game.world.bags.iter().flat_map(|b| &b.stacks).map(|s| s.count).sum();
        assert_eq!(dropped, 5);
    }

    #[test]
    fn campfire_burns_wood_and_doors_toggle() {
        let mut game = Game::new(1337);
        let here = building::tile_of(game.player.pos);
        game.world.place(StructureKind::Campfire, (here.0 + 1, here.1));
        game.world.place(StructureKind::WoodDoor, (here.0 - 1, here.1));

        game.feed_campfire(0);
        assert!(!game.world.structures[0].is_lit(), "sem madeira não acende");
        game.inventory.add(ItemId::Wood, 2);
        game.feed_campfire(0);
        assert!(game.world.structures[0].is_lit());
        assert_eq!(game.inventory.count(ItemId::Wood), 1);
        assert_eq!(game.world.lights().count(), 1);

        game.toggle_door(1);
        assert!(game.world.structures[1].open);
        game.toggle_door(1);
        assert!(!game.world.structures[1].open);
    }
}
