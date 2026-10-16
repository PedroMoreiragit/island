//! Estado do jogo e loop de update/draw. Orquestra os módulos; regras ficam neles.

use std::path::PathBuf;

use macroquad::prelude::*;

use crate::building::{self, Layer, PlaceError, StructureKind};
use crate::combat::{self, Projectile};
use crate::crafting::{CraftError, CraftQueue, RECIPES, Recipe};
use crate::creatures::{self, AiInput, Creature, CreatureSave, Event, Hit, Species};
use crate::daynight;
use crate::gathering;
use crate::inventory::{HOTBAR_SIZE, INVENTORY_SIZE, Inventory, Stack};
use crate::items::{ItemId, ToolKind};
use crate::player::{self, Facing, Player};
use crate::render::{self, POPUP_LIFETIME, Popup, PopupStyle};
use crate::rng::Rng;
use crate::save::{self, BagSave, SAVE_VERSION, SaveData};
use crate::ui::{self, ContainerRef, ContainerView, PanelKind, UiAction, UiState};
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
/// Intervalo entre tentativas de repor a fauna.
const RESPAWN_SECS: f32 = 10.0;
/// Montado, a velocidade é a corrida da espécie vezes isto (Shift multiplica de novo).
const RIDE_MULT: f32 = 1.4;
const RIDE_SPRINT: f32 = 1.3;

/// Alvo de um golpe (E / clique).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Target {
    Node(usize),
    Creature(usize),
}

/// Algo com que o jogador pode interagir com F.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Interaction {
    Bag(usize),
    Structure(usize),
    /// Dino desacordado (domesticação) ou domesticado, pelo `id`.
    Creature(u32),
}

pub struct Game {
    world: World,
    player: Player,
    vitals: Vitals,
    dead: bool,
    inventory: Inventory,
    hotbar_selected: usize,
    queue: CraftQueue,
    creatures: Vec<Creature>,
    next_creature_id: u32,
    projectiles: Vec<Projectile>,
    /// `id` da criatura que o jogador está montando.
    mount: Option<u32>,
    spawn_timer: f32,
    /// Tela piscando vermelho após levar dano (s restantes).
    hurt_flash: f32,
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
        let mut rng = Rng::new(seed ^ 0xA11CE);
        let mut creatures = Vec::new();
        let mut next_creature_id = 1;
        creatures::populate(
            &mut creatures,
            &world,
            player.pos,
            &mut rng,
            4000,
            &mut next_creature_id,
        );
        Self {
            camera_pos: player.pos,
            world,
            player,
            vitals: Vitals::full(),
            dead: false,
            inventory: Inventory::new(INVENTORY_SIZE),
            hotbar_selected: 0,
            queue: CraftQueue::default(),
            creatures,
            next_creature_id,
            projectiles: Vec::new(),
            mount: None,
            spawn_timer: RESPAWN_SECS,
            hurt_flash: 0.0,
            ui: UiState::default(),
            popups: Vec::new(),
            messages: Vec::new(),
            rng,
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
        for s in &mut game.world.structures {
            s.normalize();
        }
        // Saves anteriores à v0.4 não têm fauna: mantém a população recém-gerada.
        if let Some(saved) = data.creatures {
            game.creatures = saved.into_iter().map(Creature::from).collect();
            // Saves da v0.4 não têm ids: numera de novo.
            game.next_creature_id = game.creatures.iter().map(|c| c.id).max().unwrap_or(0) + 1;
            for c in game.creatures.iter_mut().filter(|c| c.id == 0) {
                c.id = game.next_creature_id;
                game.next_creature_id += 1;
            }
        }
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
            creatures: Some(self.creatures.iter().map(CreatureSave::from).collect()),
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
            (ItemId::RawMeat, 20),
            (ItemId::Hide, 30),
            (ItemId::Narcotic, 5),
            (ItemId::WoodenClub, 1),
            (ItemId::Slingshot, 1),
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
            let fire = &mut self.world.structures[i];
            if let Some(storage) = fire.storage.as_mut() {
                storage.add(ItemId::Wood, 5);
                storage.add(ItemId::RawMeat, 3);
            }
            fire.toggle_fire();
        }
    }

    /// Abre o painel da primeira fogueira do mundo (para screenshots).
    pub fn open_first_campfire(&mut self) {
        if let Some(i) = self
            .world
            .structures
            .iter()
            .position(|s| s.kind == StructureKind::Campfire)
        {
            self.player.pos = self.world.structures[i].rect().center() - vec2(28.0, 0.0);
            self.camera_pos = self.player.pos;
            self.ui.container = Some(ContainerRef::Structure(i));
            self.ui.inventory_open = true;
        }
    }

    /// Cena de domesticação: raptor desacordado com carne (painel aberto) para screenshots.
    pub fn demo_taming(&mut self) {
        self.creatures.clear();
        let pos = self.player.pos + vec2(30.0, 0.0);
        let mut raptor = Creature::new(Species::Raptor, pos).with_level(12);
        raptor.take_hit(4.0, 9999.0, self.player.pos);
        if let Some(inv) = raptor.inventory.as_mut() {
            inv.add(ItemId::RawMeat, 12);
            inv.add(ItemId::Narcotic, 2);
        }
        if let Some(t) = raptor.taming.as_mut() {
            t.progress = 0.35;
        }
        let id = self.add_creature(raptor);
        self.ui.container = Some(ContainerRef::Creature(id));
        self.ui.inventory_open = true;
    }

    /// Cena de montaria: jogador num raptor domesticado, um dodô seguindo e um dilo desacordado.
    pub fn demo_riding(&mut self) {
        self.creatures.clear();
        let base = self.player.pos;
        let mut raptor = Creature::new(Species::Raptor, base).with_level(18);
        raptor.command = Some(creatures::Command::Follow);
        raptor.inventory = Some(Inventory::new(Species::Raptor.pack_slots()));
        if let Some(inv) = raptor.inventory.as_mut() {
            inv.add(ItemId::RaptorSaddle, 1);
        }
        let id = self.add_creature(raptor);
        let mut dodo = Creature::new(Species::Dodo, base + vec2(-50.0, 30.0)).with_level(5);
        dodo.command = Some(creatures::Command::Follow);
        dodo.facing_left = false;
        self.add_creature(dodo);
        let mut dilo = Creature::new(Species::Dilophosaur, base + vec2(90.0, 50.0)).with_level(9);
        dilo.take_hit(0.0, 9999.0, base);
        self.add_creature(dilo);
        self.mount_creature(id);
        self.projectiles
            .push(Projectile::stone(base + vec2(20.0, -10.0), vec2(1.0, 0.3)));
    }

    /// Cena do painel de um raptor domesticado (desmontado), para screenshots.
    pub fn demo_pet_panel(&mut self) {
        self.demo_riding();
        let Some(id) = self.mount else { return };
        self.dismount();
        self.ui.container = Some(ContainerRef::Creature(id));
        self.ui.inventory_open = true;
    }

    /// Coloca uma criatura de cada espécie perto do jogador (para screenshots).
    pub fn spawn_demo_creatures(&mut self) {
        let offsets = [
            vec2(-90.0, 40.0),
            vec2(-40.0, 80.0),
            vec2(70.0, 70.0),
            vec2(120.0, 10.0),
        ];
        for (species, offset) in creatures::Species::ALL.into_iter().zip(offsets) {
            let mut c = Creature::new(species, self.player.pos + offset);
            c.facing_left = offset.x > 0.0;
            self.creatures.push(c);
        }
        let mut corpse = Creature::new(creatures::Species::Dodo, self.player.pos + vec2(40.0, -50.0));
        corpse.take_hit(999.0, 0.0, self.player.pos);
        self.creatures.push(corpse);
        // Dilofossauro ferido, para mostrar a barra de vida.
        let dilo = self.creatures.len() - 3;
        self.creatures[dilo].take_hit(30.0, 20.0, self.player.pos);
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
        self.popup_styled(pos, text, PopupStyle::Gain);
    }

    fn popup_styled(&mut self, pos: Vec2, text: String, style: PopupStyle) {
        self.popups.push(Popup {
            pos,
            text,
            style,
            age: 0.0,
        });
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

    fn container_mut(&mut self) -> Option<&mut Inventory> {
        match self.ui.container? {
            ContainerRef::Structure(i) => self.world.structures.get_mut(i)?.storage.as_mut(),
            ContainerRef::Creature(id) => self.creature_mut(id)?.inventory.as_mut(),
        }
    }

    fn creature(&self, id: u32) -> Option<&Creature> {
        self.creatures.iter().find(|c| c.id == id)
    }

    fn creature_mut(&mut self, id: u32) -> Option<&mut Creature> {
        self.creatures.iter_mut().find(|c| c.id == id)
    }

    /// Adiciona uma criatura com um `id` novo.
    fn add_creature(&mut self, mut c: Creature) -> u32 {
        c.id = self.next_creature_id;
        self.next_creature_id += 1;
        let id = c.id;
        self.creatures.push(c);
        id
    }

    fn mounted(&self) -> Option<&Creature> {
        self.creature(self.mount?)
    }

    /// Posição do mouse no mundo.
    fn mouse_world(&self) -> Vec2 {
        self.camera().screen_to_world(mouse_position().into())
    }

    fn set_inventory_open(&mut self, open: bool) {
        self.ui.inventory_open = open;
        if open {
            return;
        }
        // O que não couber de volta no inventário vai para o baú aberto ou para o chão.
        if let Some(left) = ui::return_held(&mut self.ui, &mut self.inventory) {
            let left = match self.container_mut() {
                Some(chest) => chest.insert_stack(left),
                None => Some(left),
            };
            if let Some(stack) = left {
                self.drop_stack(stack);
            }
        }
        self.ui.container = None;
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
        self.update_creatures(dt);
        self.validate_open_container();

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

        let container = match self.ui.container {
            Some(ContainerRef::Structure(i)) => self.world.structures.get_mut(i).and_then(|s| {
                let kind = if s.kind == StructureKind::Campfire {
                    PanelKind::Campfire
                } else {
                    PanelKind::Storage
                };
                s.storage.as_mut().map(|inv| (kind, inv))
            }),
            Some(ContainerRef::Creature(id)) => self.creatures.iter_mut().find(|c| c.id == id).and_then(|c| {
                let kind = if c.is_tamed() {
                    PanelKind::Tamed
                } else {
                    PanelKind::Taming
                };
                c.inventory.as_mut().map(|inv| (kind, inv))
            }),
            None => None,
        };
        match ui::handle_input(&mut self.ui, &mut self.inventory, container, &self.queue) {
            Some(UiAction::Craft(i)) => self.craft(i),
            Some(UiAction::CancelCraft(i)) => {
                for stack in self.queue.cancel(i, &mut self.inventory) {
                    self.drop_stack(stack);
                }
            }
            Some(UiAction::SelectHotbar(i)) => self.hotbar_selected = i,
            Some(UiAction::Drop(stack)) => self.drop_stack(stack),
            Some(UiAction::ToggleFire) => self.toggle_fire(),
            Some(UiAction::ToggleCommand) => {
                if let Some(ContainerRef::Creature(id)) = self.ui.container
                    && let Some(c) = self.creature_mut(id)
                {
                    c.toggle_command();
                }
            }
            Some(UiAction::Mount) => {
                if let Some(ContainerRef::Creature(id)) = self.ui.container {
                    self.set_inventory_open(false);
                    self.mount_creature(id);
                }
            }
            None => {}
        }

        let free = !self.ui.inventory_open;
        let input = if free { movement_input() } else { Vec2::ZERO };
        if self.mount.is_some() {
            self.update_riding(input, free, dt);
            return;
        }
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

    /// Fecha o painel aberto se o contêiner sumiu ou se o jogador se afastou.
    fn validate_open_container(&mut self) {
        let Some(container) = self.ui.container else { return };
        let reach = building::INTERACT_REACH + 16.0;
        let p = self.player.pos;
        let still_valid = match container {
            ContainerRef::Structure(i) => self
                .world
                .structures
                .get(i)
                .is_some_and(|s| s.storage.is_some() && building::distance_to_rect(p, s.rect()) <= reach),
            ContainerRef::Creature(id) => self.creature(id).is_some_and(|c| {
                let usable = c.is_tamed() || (c.knocked_out && c.inventory.is_some());
                usable && c.is_alive() && c.pos.distance(p) - c.species.radius() <= reach + 8.0
            }),
        };
        if !still_valid || self.dead {
            self.set_inventory_open(false);
        }
    }

    // ---------- Fauna ----------

    fn update_creatures(&mut self, dt: f32) {
        let input = AiInput {
            player_pos: self.player.pos,
            player_alive: !self.dead,
        };
        let mut damage = 0.0;
        let mut events = Vec::new();
        for c in &mut self.creatures {
            match c.update(&self.world, &input, &mut self.rng, dt) {
                Some(Event::Bite(d)) => damage += d,
                Some(event) => events.push((c.id, event)),
                None => {}
            }
        }
        for (id, event) in events {
            self.on_creature_event(id, event);
        }
        self.update_projectiles(dt);
        self.creatures.retain(|c| !c.is_gone());
        if self.mount.is_some() && self.mounted().is_none_or(|m| !m.is_alive()) {
            self.mount = None;
        }

        self.hurt_flash = (self.hurt_flash - dt).max(0.0);
        if damage > 0.0 && !self.dead {
            self.take_bite(damage);
        }

        self.spawn_timer -= dt;
        if self.spawn_timer <= 0.0 {
            self.spawn_timer = RESPAWN_SECS;
            creatures::populate(
                &mut self.creatures,
                &self.world,
                self.player.pos,
                &mut self.rng,
                40,
                &mut self.next_creature_id,
            );
        }
    }

    /// Mordidas de selvagens: montado, quem apanha é a montaria.
    fn take_bite(&mut self, damage: f32) {
        let player_pos = self.player.pos;
        if let Some(id) = self.mount
            && let Some(mount) = self.creature_mut(id)
        {
            let died = mount.take_hit(damage, 0.0, player_pos) == Hit::Died;
            let (pos, name) = (mount.pos, mount.species.name());
            self.popup_styled(pos - vec2(0.0, 30.0), format!("-{damage:.0}"), PopupStyle::Hurt);
            if died {
                self.dismount();
                self.on_tamed_death(id);
                self.notify(format!("Seu {name} morreu!"));
            }
            return;
        }
        self.vitals.damage(damage);
        self.hurt_flash = 0.35;
        let at = self.player.pos - vec2(0.0, 36.0);
        self.popup_styled(at, format!("-{damage:.0}"), PopupStyle::Hurt);
        if self.vitals.is_dead() {
            self.die();
        }
    }

    /// Inventário de um domesticado morto cai no chão.
    fn on_tamed_death(&mut self, id: u32) {
        if let Some(c) = self.creature_mut(id) {
            let (pos, stacks) = (c.pos, c.take_inventory());
            self.world.drop_bag(pos, stacks, BagKind::Dropped);
        }
    }

    fn on_creature_event(&mut self, id: u32, event: Event) {
        let Some(c) = self.creature_mut(id) else { return };
        let (name, pos) = (c.species.name(), c.pos);
        match event {
            Event::Woke => {
                // A comida que sobrou cai no chão.
                let stacks = c.take_inventory();
                c.inventory = None;
                self.world.drop_bag(pos, stacks, BagKind::Dropped);
                self.notify(format!("{name} acordou!"));
            }
            Event::Tamed { bonus_levels } => {
                let level = c.level;
                self.notify(format!("{name} domesticado! Nv {level} (+{bonus_levels} de bônus)"));
                self.popup(pos - vec2(0.0, 30.0), "Domesticado!".to_owned());
            }
            Event::Bite(_) => {}
        }
    }

    fn update_projectiles(&mut self, dt: f32) {
        let player_pos = self.player.pos;
        let mut hits = Vec::new();
        // Subpassos curtos para a pedra não atravessar alvos quando o frame demora.
        let substeps = (dt * combat::SLINGSHOT_SPEED / 4.0).ceil().max(1.0) as usize;
        let sub_dt = dt / substeps as f32;
        self.projectiles.retain_mut(|stone| {
            for _ in 0..substeps {
                if !stone.update(&self.world, sub_dt) {
                    return false;
                }
                let target = self
                    .creatures
                    .iter()
                    .position(|c| c.is_wild() && c.is_conscious() && stone.hits(c.pos, c.species.radius()));
                if let Some(i) = target {
                    hits.push((i, stone.damage, stone.torpor));
                    return false;
                }
            }
            true
        });
        for (i, damage, torpor) in hits {
            let hit = self.creatures[i].take_hit(damage, torpor, player_pos);
            self.report_hit(i, damage, hit);
        }
    }

    /// Mensagens e números flutuantes de um golpe numa criatura.
    fn report_hit(&mut self, i: usize, damage: f32, hit: Hit) {
        let c = &self.creatures[i];
        let (pos, species) = (c.pos, c.species);
        if damage > 0.0 {
            self.popup_styled(pos - vec2(0.0, 30.0), format!("-{damage:.0}"), PopupStyle::Damage);
        }
        match hit {
            Hit::Died => self.notify(format!("{} abatido", species.name())),
            Hit::KnockedOut if species.taming_food().is_some() => {
                self.notify(format!("{} desmaiou! Aperte F nele e coloque comida.", species.name()));
            }
            Hit::KnockedOut => self.notify(format!("{} desmaiou", species.name())),
            Hit::Hurt => {}
        }
    }

    // ---------- Montaria ----------

    fn mount_creature(&mut self, id: u32) {
        let Some(c) = self.creature_mut(id) else { return };
        if !c.can_ride() {
            return;
        }
        c.mounted = true;
        let pos = c.pos;
        self.mount = Some(id);
        self.player.pos = pos;
        self.camera_pos = pos;
    }

    /// Desce da montaria para um lugar livre ao lado dela.
    fn dismount(&mut self) {
        let Some(id) = self.mount.take() else { return };
        let Some(c) = self.creature_mut(id) else { return };
        c.mounted = false;
        let (pos, r) = (c.pos, c.species.radius());
        let side = if c.facing_left { 1.0 } else { -1.0 };
        let spots = [vec2(side, 0.0), vec2(-side, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)];
        let free = spots
            .iter()
            .map(|d| pos + *d * (r + player::RADIUS + 4.0))
            .find(|&p| !self.world.blocked(p, player::RADIUS));
        self.player.pos = free.unwrap_or(pos);
    }

    fn update_riding(&mut self, input: Vec2, free: bool, dt: f32) {
        let Some(id) = self.mount else { return };
        let sprint = free && is_key_down(KeyCode::LeftShift);
        let world = &self.world;
        let Some(mount) = self.creatures.iter_mut().find(|c| c.id == id) else {
            self.mount = None;
            return;
        };
        let speed = mount.species.speeds().1 * RIDE_MULT * if sprint { RIDE_SPRINT } else { 1.0 };
        mount.ride(world, input, speed, dt);
        self.player.pos = mount.pos;
        self.player.facing = if mount.facing_left { Facing::Left } else { Facing::Right };
        self.player.update(Vec2::ZERO, 1.0, world, dt);
        self.vitals.tick(dt, false);

        if free {
            let over_ui = ui::pointer_over_ui(&self.ui);
            if is_key_down(KeyCode::E) || (is_mouse_button_down(MouseButton::Left) && !over_ui) {
                self.mount_bite();
            }
            if is_key_pressed(KeyCode::F) {
                self.dismount();
            }
            if is_key_pressed(KeyCode::R) || (is_mouse_button_pressed(MouseButton::Right) && !over_ui) {
                self.use_selected();
            }
        }
        if self.vitals.is_dead() {
            self.die();
        }
    }

    /// A montaria morde o selvagem mais próximo à frente dela.
    fn mount_bite(&mut self) {
        let Some(mount) = self.mounted() else { return };
        if !mount.ready_to_bite() {
            return;
        }
        let (pos, reach, s) = (
            mount.pos,
            mount.species.radius() + 18.0,
            if mount.facing_left { -1.0 } else { 1.0 },
        );
        let target = self
            .creatures
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_wild() && c.is_conscious())
            .map(|(i, c)| {
                (
                    i,
                    c.pos.distance(pos) - c.species.radius(),
                    (c.pos.x - pos.x) * s >= -4.0,
                )
            })
            .filter(|&(_, gap, ahead)| gap <= reach && ahead)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _, _)| i);
        let Some(id) = self.mount else { return };
        let damage = match self.creature_mut(id) {
            Some(m) => m.bite(),
            None => return,
        };
        if let Some(i) = target {
            let hit = self.creatures[i].take_hit(damage, 0.0, pos);
            self.report_hit(i, damage, hit);
        }
    }

    // ---------- Coleta, combate, comida, água ----------

    /// Alvo mais próximo ao alcance, preferindo o que está à frente do personagem.
    /// Criaturas vivas têm prioridade sobre recursos; carcaças contam como recurso.
    fn find_target(&self) -> Option<Target> {
        let p = &self.player;
        let weapon_reach = player::REACH + ItemId::reach_bonus(self.equipped());
        let score = |pos: Vec2, radius: f32, reach: f32| {
            let gap = pos.distance(p.pos) - radius;
            let behind = (pos.x - p.pos.x) * p.facing.sign() < -4.0;
            (gap <= reach).then_some(gap + if behind { 20.0 } else { 0.0 })
        };
        let nodes = self
            .world
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_alive())
            .filter_map(|(i, n)| score(n.pos, n.kind.collision_radius(), player::REACH).map(|s| (s, Target::Node(i))));
        // Domesticados não são alvo; desmaiados também não (bater reduz a efetividade).
        let creatures = self
            .creatures
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_wild() && !c.knocked_out)
            .filter_map(|(i, c)| {
                let (reach, bonus) = if c.is_alive() {
                    (weapon_reach, 10.0)
                } else {
                    (player::REACH, 0.0)
                };
                score(c.pos, c.species.radius(), reach).map(|s| (s - bonus, Target::Creature(i)))
            });
        nodes
            .chain(creatures)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, t)| t)
    }

    /// Posição e raio do alvo atual, para o anel de destaque.
    fn target_ring(&self, target: Target) -> (Vec2, f32) {
        match target {
            Target::Node(i) => {
                let n = &self.world.nodes[i];
                (n.pos, n.kind.collision_radius().max(8.0) + 6.0)
            }
            Target::Creature(i) => {
                let c = &self.creatures[i];
                (c.pos, c.species.radius() + 6.0)
            }
        }
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
        if self.equipped() == Some(ItemId::Slingshot) {
            self.fire_slingshot();
            return;
        }
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
        match target {
            Some(Target::Node(i)) => self.hit_node(i),
            Some(Target::Creature(i)) => self.hit_creature(i),
            None => {}
        }
    }

    /// Dispara uma pedra na direção do cursor (clique) ou para a frente (E).
    fn fire_slingshot(&mut self) {
        self.player.cooldown = combat::SLINGSHOT_COOLDOWN;
        if self.inventory.count(ItemId::Stone) == 0 {
            self.notify("Sem pedras para o estilingue");
            return;
        }
        if !self.vitals.spend_stamina(vitals::SWING_STAMINA) {
            self.notify("Sem stamina!");
            return;
        }
        let from = self.player.pos;
        let dir = if is_key_down(KeyCode::E) {
            vec2(self.player.facing.sign(), 0.0)
        } else {
            self.mouse_world() + vec2(0.0, 14.0) - from
        };
        self.player.face_towards(from.x + dir.x);
        self.inventory.remove(ItemId::Stone, 1);
        self.projectiles.push(Projectile::stone(from, dir));
        self.player.swing_timer = player::SWING_TIME;
        self.wear_equipped();
    }

    /// Gasta durabilidade do item na mão (se ele tiver) e avisa se quebrou.
    fn wear_equipped(&mut self) {
        if let Some(broken) = self.inventory.wear(self.hotbar_selected) {
            self.notify(format!("{} quebrou!", broken.name()));
        }
    }

    fn hit_creature(&mut self, i: usize) {
        let held = self.equipped();
        let tool = self.equipped_tool();
        let damage = ItemId::melee_damage(held);
        let torpor = ItemId::melee_torpor(held);
        let player_pos = self.player.pos;
        let c = &mut self.creatures[i];
        let pos = c.pos;
        let species = c.species;
        if c.is_alive() {
            let hit = c.take_hit(damage, torpor, player_pos);
            self.player.face_towards(pos.x);
            self.report_hit(i, damage, hit);
            if held.is_some_and(|item| item.max_durability().is_some()) {
                self.wear_equipped();
            }
            return;
        }
        if !c.harvest() {
            return;
        }
        self.player.face_towards(pos.x);
        if matches!(tool, Some(ToolKind::Pickaxe | ToolKind::Hatchet)) {
            self.wear_equipped();
        }
        let drops = gathering::roll_corpse_yield(species, tool, &mut self.rng);
        self.give_drops(pos, drops);
    }

    fn hit_node(&mut self, idx: usize) {
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
        if matches!(tool, Some(ToolKind::Pickaxe | ToolKind::Hatchet)) {
            self.wear_equipped();
        }
        let drops = gathering::roll_yield(kind, tool, &mut self.rng);
        self.give_drops(pos, drops);
    }

    /// Coloca os itens coletados no inventário, com números flutuantes sobre `pos`.
    fn give_drops(&mut self, pos: Vec2, drops: Vec<(ItemId, u32)>) {
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
        if effect.health < 0.0 {
            self.notify("Carne crua faz mal. Cozinhe na fogueira!");
        }
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
        let creature = self
            .creatures
            .iter()
            .filter(|c| c.is_alive() && (c.is_tamed() || (c.knocked_out && c.inventory.is_some())))
            .map(|c| (c.pos.distance(p) - c.species.radius(), Interaction::Creature(c.id)))
            .filter(|(d, _)| *d <= building::INTERACT_REACH + 6.0)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        [bag, structure, creature]
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
                StructureKind::StorageBox | StructureKind::Campfire => {
                    self.ui.container = Some(ContainerRef::Structure(i));
                    self.ui.inventory_open = true;
                }
                StructureKind::Workbench => self.set_inventory_open(true),
                StructureKind::WoodFoundation | StructureKind::WoodWall => {}
            },
            Interaction::Creature(id) => {
                self.ui.container = Some(ContainerRef::Creature(id));
                self.ui.inventory_open = true;
            }
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

    /// Acende/apaga a fogueira aberta no painel.
    fn toggle_fire(&mut self) {
        let Some(ContainerRef::Structure(i)) = self.ui.container else {
            return;
        };
        let Some(fire) = self.world.structures.get_mut(i) else {
            return;
        };
        if !fire.toggle_fire() {
            self.notify("Coloque madeira ou palha na fogueira");
        }
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
        self.dismount();
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
        let on_foot = interactive && self.mount.is_none();
        let ring = on_foot
            .then(|| self.find_target())
            .flatten()
            .map(|t| self.target_ring(t));
        render::draw_entities(&render::Scene {
            world: &self.world,
            creatures: &self.creatures,
            projectiles: &self.projectiles,
            player: (!self.dead).then_some(&self.player),
            held: self.equipped(),
            mount: self.mount,
            ring,
            view,
            time: self.time,
        });

        let hour = daynight::hour(self.time);
        let flicker = 1.0 + (self.time * 9.0).sin() * 0.03 + (self.time * 23.0).sin() * 0.02;
        let mut lights: Vec<(Vec2, f32, bool)> = self.world.lights().map(|(p, r)| (p, r * flicker, true)).collect();
        if !self.dead {
            lights.push((self.player.pos - vec2(0.0, 12.0), PLAYER_LIGHT, false));
        }
        render::draw_golden_hour(view, daynight::golden_hour(hour));
        render::draw_lighting(view, daynight::darkness(hour), &lights);

        if on_foot && let Some(kind) = self.placing_kind() {
            let tile = self.mouse_tile();
            let valid = self
                .world
                .can_place(kind, tile, self.player.pos, player::RADIUS)
                .is_ok();
            render::draw_ghost(kind, tile, valid);
        }

        set_default_camera();
        render::draw_creature_bars(&self.creatures, &camera, self.mount);
        render::draw_popups(&self.popups, &camera);
        if on_foot && let Some(target) = self.nearest_interaction() {
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
                Interaction::Creature(id) => match self.creature(id) {
                    Some(c) if c.knocked_out => (
                        format!("[F] Alimentar {} (desacordado)", c.species.name()),
                        c.pos - vec2(0.0, 44.0),
                    ),
                    Some(c) => (
                        format!("[F] {} Nv {}", c.species.name(), c.level),
                        c.pos - vec2(0.0, c.species.radius() * 2.8 + 24.0),
                    ),
                    None => (String::new(), Vec2::ZERO),
                },
            };
            render::draw_prompt(&label, pos, &camera);
        }

        let container = match self.ui.container {
            Some(ContainerRef::Structure(i)) => self.world.structures.get(i).map(ContainerView::Structure),
            Some(ContainerRef::Creature(id)) => self.creature(id).map(ContainerView::Creature),
            None => None,
        };
        ui::draw(&ui::Hud {
            ui: &self.ui,
            inventory: &self.inventory,
            container,
            mount: self.mounted(),
            hurt: self.hurt_flash,
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
    fn campfire_panel_and_doors() {
        let mut game = Game::new(1337);
        let here = building::tile_of(game.player.pos);
        game.world.place(StructureKind::Campfire, (here.0 + 1, here.1));
        game.world.place(StructureKind::WoodDoor, (here.0 - 1, here.1));

        game.interact(Interaction::Structure(0));
        assert_eq!(game.ui.container, Some(ContainerRef::Structure(0)));
        game.toggle_fire();
        assert!(!game.world.structures[0].is_lit(), "sem combustível não acende");
        game.world.structures[0].storage.as_mut().unwrap().add(ItemId::Wood, 2);
        game.toggle_fire();
        assert!(game.world.structures[0].is_lit());
        assert_eq!(game.world.lights().count(), 1);

        game.toggle_door(1);
        assert!(game.world.structures[1].open);
        game.toggle_door(1);
        assert!(!game.world.structures[1].open);
    }

    #[test]
    fn kill_and_harvest_a_dodo() {
        let mut game = Game::new(1337);
        game.creatures.clear();
        game.inventory.add(ItemId::WoodenSpear, 1);
        let pos = game.player.pos + vec2(20.0, 0.0);
        game.creatures.push(Creature::new(creatures::Species::Dodo, pos));

        assert_eq!(game.find_target(), Some(Target::Creature(0)));
        game.hit_creature(0);
        game.hit_creature(0);
        assert!(!game.creatures[0].is_alive());
        assert_eq!(game.inventory.slots[0].unwrap().durability, 48, "a lança desgasta");

        for _ in 0..creatures::Species::Dodo.harvest_hits() {
            game.hit_creature(0);
        }
        assert!(game.inventory.count(ItemId::RawMeat) >= 3);
        game.update_creatures(0.0);
        assert!(game.creatures.is_empty(), "carcaça esgotada some");
    }

    #[test]
    fn club_knocks_out_then_feeding_tames() {
        let mut game = Game::new(1337);
        game.creatures.clear();
        game.inventory.add(ItemId::WoodenClub, 1);
        let pos = game.player.pos + vec2(20.0, 0.0);
        let id = game.add_creature(Creature::new(Species::Dodo, pos));

        let mut swings = 0;
        while !game.creature(id).unwrap().knocked_out {
            game.hit_creature(0);
            swings += 1;
            assert!(swings < 10, "a clava deveria derrubar o dodô");
        }
        assert_eq!(game.find_target(), None, "desmaiado não é alvo de golpe");
        assert_eq!(game.nearest_interaction(), Some(Interaction::Creature(id)));

        game.interact(Interaction::Creature(id));
        assert_eq!(game.ui.container, Some(ContainerRef::Creature(id)));
        game.container_mut().unwrap().add(ItemId::Berry, 10);
        for _ in 0..60 {
            game.update_creatures(1.0);
        }
        let dodo = game.creature(id).unwrap();
        assert!(dodo.is_tamed(), "deveria estar domesticado");
        assert_eq!(game.find_target(), None, "domesticado não é alvo");
    }

    #[test]
    fn slingshot_stone_adds_torpor() {
        let mut game = Game::new(1337);
        game.creatures.clear();
        let pos = game.player.pos + vec2(60.0, 0.0);
        let id = game.add_creature(Creature::new(Species::Raptor, pos));
        game.projectiles
            .push(Projectile::stone(game.player.pos, vec2(1.0, 0.0)));
        game.update_projectiles(0.2);
        assert!(game.projectiles.is_empty(), "a pedra acertou e sumiu");
        assert_eq!(game.creature(id).unwrap().torpor, combat::SLINGSHOT_TORPOR);
    }

    #[test]
    fn riding_moves_together_and_mount_takes_bites() {
        let mut game = Game::new(1337);
        game.creatures.clear();
        let mut raptor = Creature::new(Species::Raptor, game.player.pos + vec2(20.0, 0.0));
        raptor.command = Some(creatures::Command::Follow);
        raptor.inventory = Some(Inventory::new(Species::Raptor.pack_slots()));
        let id = game.add_creature(raptor);

        game.mount_creature(id);
        assert_eq!(game.mount, None, "sem sela não monta");
        game.creature_mut(id)
            .unwrap()
            .inventory
            .as_mut()
            .unwrap()
            .add(ItemId::RaptorSaddle, 1);
        game.mount_creature(id);
        assert_eq!(game.mount, Some(id));
        assert_eq!(game.player.pos, game.creature(id).unwrap().pos);

        let health = game.vitals.health;
        game.take_bite(10.0);
        assert_eq!(game.vitals.health, health, "a montaria protege o jogador");
        assert!(game.creature(id).unwrap().health < game.creature(id).unwrap().max_health());

        game.dismount();
        assert_eq!(game.mount, None);
        assert!(!game.creature(id).unwrap().mounted);
        assert_ne!(game.player.pos, game.creature(id).unwrap().pos);
    }

    #[test]
    fn creature_bites_can_kill_the_player() {
        let mut game = Game::new(1337);
        game.creatures.clear();
        game.inventory.add(ItemId::Stone, 3);
        let pos = game.player.pos + vec2(22.0, 0.0);
        game.creatures.push(Creature::new(creatures::Species::Raptor, pos));
        game.vitals.health = 10.0;
        for _ in 0..240 {
            game.update_creatures(1.0 / 60.0);
        }
        assert!(game.dead);
        assert!(game.world.bags.iter().any(|b| b.kind == BagKind::Death));
    }
}
