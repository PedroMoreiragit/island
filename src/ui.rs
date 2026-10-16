//! HUD (hotbar, status, relógio, mensagens) e painéis de inventário, crafting e baú em modo imediato.
//! Layout é recalculado a cada frame pelas mesmas funções usadas no input e no desenho.

use macroquad::prelude::*;

use crate::building::{COOK_SECS, Structure, StructureKind};
use crate::crafting::{self, CraftQueue, MAX_QUEUE, RECIPES, Recipe};
use crate::creatures::{Command, Creature};
use crate::daynight;
use crate::inventory::{HOTBAR_SIZE, Inventory, Stack};
use crate::items::ItemId;
use crate::taming;
use crate::vitals::{MAX_FOOD, MAX_HEALTH, MAX_STAMINA, MAX_WATER, Vitals};

const SLOT: f32 = 52.0;
const SMALL_SLOT: f32 = 40.0;
const GAP: f32 = 6.0;
const PAD: f32 = 16.0;
const HEADER: f32 = 36.0;
const COLS: usize = 6;
const RIGHT_W: f32 = 400.0;
const DETAIL_H: f32 = 92.0;

const PANEL_BG: Color = Color::new(0.10, 0.11, 0.13, 0.92);
const SLOT_BG: Color = Color::new(1.0, 1.0, 1.0, 0.08);
const BORDER: Color = Color::new(1.0, 1.0, 1.0, 0.18);
const ACCENT: Color = Color::new(1.0, 0.80, 0.35, 1.0);
const TEXT: Color = Color::new(0.93, 0.93, 0.93, 1.0);
const DIM: Color = Color::new(0.65, 0.65, 0.68, 1.0);
const OK: Color = Color::new(0.55, 0.85, 0.50, 1.0);
const BAD: Color = Color::new(0.95, 0.45, 0.40, 1.0);

#[derive(Default)]
pub struct UiState {
    pub inventory_open: bool,
    /// Pilha "presa" ao cursor enquanto o jogador reorganiza o inventário.
    pub held: Option<Stack>,
    /// Contêiner (baú, fogueira, dino) aberto junto com o inventário.
    pub container: Option<ContainerRef>,
    pub hide_help: bool,
}

/// O que está aberto ao lado do inventário.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContainerRef {
    /// Índice em `world.structures`.
    Structure(usize),
    /// `id` da criatura.
    Creature(u32),
}

/// Tipo de painel de contêiner: define layout e botões.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    Storage,
    Campfire,
    /// Dino desacordado sendo domesticado.
    Taming,
    /// Dino domesticado.
    Tamed,
}

impl PanelKind {
    /// Altura extra abaixo dos slots (status, barras, botões).
    fn extra_height(self) -> f32 {
        match self {
            PanelKind::Storage => 0.0,
            PanelKind::Campfire => 84.0,
            PanelKind::Taming | PanelKind::Tamed => 112.0,
        }
    }

    fn buttons(self) -> &'static [UiAction] {
        match self {
            PanelKind::Campfire => &[UiAction::ToggleFire],
            PanelKind::Tamed => &[UiAction::ToggleCommand, UiAction::Mount],
            PanelKind::Storage | PanelKind::Taming => &[],
        }
    }
}

/// Leitura do contêiner aberto, para desenhar o painel.
#[derive(Clone, Copy)]
pub enum ContainerView<'a> {
    Structure(&'a Structure),
    Creature(&'a Creature),
}

impl<'a> ContainerView<'a> {
    fn kind(self) -> PanelKind {
        match self {
            ContainerView::Structure(s) if s.kind == StructureKind::Campfire => PanelKind::Campfire,
            ContainerView::Structure(_) => PanelKind::Storage,
            ContainerView::Creature(c) if c.is_tamed() => PanelKind::Tamed,
            ContainerView::Creature(_) => PanelKind::Taming,
        }
    }

    fn inventory(self) -> Option<&'a Inventory> {
        match self {
            ContainerView::Structure(s) => s.storage.as_ref(),
            ContainerView::Creature(c) => c.inventory.as_ref(),
        }
    }

    fn title(self) -> String {
        match self {
            ContainerView::Structure(s) => s.kind.item().name().to_owned(),
            ContainerView::Creature(c) => format!("{} Nv {}", c.species.name(), c.level),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Craft(usize),
    CancelCraft(usize),
    SelectHotbar(usize),
    /// Pilha solta fora dos painéis: vai para o chão.
    Drop(Stack),
    /// Acender/apagar a fogueira aberta.
    ToggleFire,
    /// Alternar seguir/ficar do dino aberto.
    ToggleCommand,
    /// Montar no dino aberto.
    Mount,
}

/// Tudo o que o HUD precisa ler do jogo.
pub struct Hud<'a> {
    pub ui: &'a UiState,
    pub inventory: &'a Inventory,
    pub container: Option<ContainerView<'a>>,
    /// Montaria atual, para a barra de vida dela.
    pub mount: Option<&'a Creature>,
    /// Flash vermelho de dano recebido (s restantes).
    pub hurt: f32,
    pub queue: &'a CraftQueue,
    /// Receitas que pedem estação estão liberadas (estação por perto)?
    pub station_ok: &'a dyn Fn(&Recipe) -> bool,
    pub selected: usize,
    pub messages: &'a [(String, f32)],
    pub vitals: &'a Vitals,
    pub dead: bool,
    pub time: f32,
}

// ---------- Layout ----------

enum RightLayout {
    Craft {
        recipes: Vec<Rect>,
        detail: Rect,
        queue: Vec<Rect>,
    },
    Container {
        slots: Vec<Rect>,
        /// Área abaixo dos slots para status/barras.
        info: Rect,
        /// Botões, na ordem de `PanelKind::buttons`.
        buttons: Vec<Rect>,
    },
}

struct PanelLayout {
    inv_panel: Rect,
    slots: Vec<Rect>,
    right_panel: Rect,
    right: RightLayout,
}

fn grid(origin: Vec2, n: usize, cols: usize, size: f32) -> Vec<Rect> {
    (0..n)
        .map(|i| {
            let (c, r) = ((i % cols) as f32, (i / cols) as f32);
            Rect::new(origin.x + c * (size + GAP), origin.y + r * (size + GAP), size, size)
        })
        .collect()
}

fn grid_height(n: usize, cols: usize, size: f32) -> f32 {
    n.div_ceil(cols) as f32 * (size + GAP) - GAP
}

/// `container`: (slots, tipo) do contêiner aberto, se houver (substitui o painel de crafting).
fn panel_layout(n_slots: usize, container: Option<(usize, PanelKind)>, queue_len: usize) -> PanelLayout {
    let grid_w = COLS as f32 * (SLOT + GAP) - GAP;
    let inv_w = grid_w + PAD * 2.0;
    let inv_h = HEADER + PAD * 2.0 + grid_height(n_slots, COLS, SLOT) + 8.0;
    let right_h = match container {
        Some((n, kind)) => HEADER + PAD * 2.0 + grid_height(n, COLS, SLOT) + 24.0 + kind.extra_height(),
        None => {
            HEADER + PAD * 2.0 + grid_height(RECIPES.len(), COLS, SLOT) + DETAIL_H + 24.0 + 22.0 + SMALL_SLOT + 48.0
        }
    };
    let total_w = inv_w + 16.0 + RIGHT_W;
    let x0 = ((screen_width() - total_w) / 2.0).max(8.0);
    let y0 = ((screen_height() - inv_h.max(right_h)) / 2.0 - 30.0).max(8.0);

    let mut slots = grid(vec2(x0 + PAD, y0 + HEADER + PAD), n_slots, COLS, SLOT);
    // Primeira linha (hotbar) levemente separada do resto.
    for r in slots.iter_mut().skip(COLS) {
        r.y += 8.0;
    }

    let rx = x0 + inv_w + 16.0;
    let top = vec2(rx + PAD, y0 + HEADER + PAD);
    let right = match container {
        Some((n, kind)) => {
            let slots = grid(top, n, COLS, SLOT);
            let info_y = top.y + grid_height(n, COLS, SLOT) + 8.0;
            let info = Rect::new(top.x, info_y, RIGHT_W - PAD * 2.0, kind.extra_height());
            let buttons = (0..kind.buttons().len())
                .map(|i| Rect::new(top.x + i as f32 * 132.0, info.bottom() - 40.0, 120.0, 32.0))
                .collect();
            RightLayout::Container { slots, info, buttons }
        }
        None => {
            let recipes = grid(top, RECIPES.len(), COLS, SLOT);
            let detail_y = top.y + grid_height(RECIPES.len(), COLS, SLOT) + 12.0;
            let detail = Rect::new(top.x, detail_y, RIGHT_W - PAD * 2.0, DETAIL_H);
            let queue_y = detail.bottom() + 12.0 + 22.0;
            let queue = grid(vec2(top.x, queue_y), queue_len, MAX_QUEUE, SMALL_SLOT);
            RightLayout::Craft { recipes, detail, queue }
        }
    };

    PanelLayout {
        inv_panel: Rect::new(x0, y0, inv_w, inv_h),
        slots,
        right_panel: Rect::new(rx, y0, RIGHT_W, right_h),
        right,
    }
}

fn hotbar_slots() -> Vec<Rect> {
    let w = HOTBAR_SIZE as f32 * (SLOT + GAP) - GAP;
    let x0 = (screen_width() - w) / 2.0;
    let y = screen_height() - SLOT - 16.0;
    (0..HOTBAR_SIZE)
        .map(|i| Rect::new(x0 + i as f32 * (SLOT + GAP), y, SLOT, SLOT))
        .collect()
}

fn mouse() -> Vec2 {
    mouse_position().into()
}

// ---------- Input ----------

/// O cursor está sobre algum elemento de UI (para não coletar ao clicar na hotbar)?
pub fn pointer_over_ui(ui: &UiState) -> bool {
    let m = mouse();
    ui.inventory_open || hotbar_slots().iter().any(|r| r.contains(m))
}

/// Devolve ao inventário a pilha presa no cursor. Retorna o que não coube.
pub fn return_held(ui: &mut UiState, inv: &mut Inventory) -> Option<Stack> {
    ui.held.take().and_then(|stack| inv.insert_stack(stack))
}

pub fn handle_input(
    ui: &mut UiState,
    inv: &mut Inventory,
    container: Option<(PanelKind, &mut Inventory)>,
    queue: &CraftQueue,
) -> Option<UiAction> {
    let kind = container.as_ref().map(|c| c.0);
    let mut chest = container.map(|c| c.1);
    let m = mouse();
    if !ui.inventory_open {
        if is_mouse_button_pressed(MouseButton::Left) {
            return hotbar_slots()
                .iter()
                .position(|r| r.contains(m))
                .map(UiAction::SelectHotbar);
        }
        return None;
    }

    let layout = panel_layout(
        inv.slots.len(),
        chest.as_ref().zip(kind).map(|(c, k)| (c.slots.len(), k)),
        queue.jobs.len(),
    );
    let left = is_mouse_button_pressed(MouseButton::Left);
    let right = is_mouse_button_pressed(MouseButton::Right);
    let shift = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);

    if let Some(i) = layout.slots.iter().position(|r| r.contains(m)) {
        match chest.as_deref_mut() {
            Some(chest) if left && shift && ui.held.is_none() => transfer(&mut inv.slots[i], chest),
            _ => slot_click(ui, &mut inv.slots[i], left, right),
        }
        return None;
    }

    match &layout.right {
        RightLayout::Container { slots, buttons, .. } => {
            if left
                && let Some(i) = buttons.iter().position(|b| b.contains(m))
                && let Some(kind) = kind
            {
                return Some(kind.buttons()[i]);
            }
            if let (Some(chest), Some(i)) = (chest, slots.iter().position(|r| r.contains(m))) {
                if left && shift && ui.held.is_none() {
                    transfer(&mut chest.slots[i], inv);
                } else {
                    slot_click(ui, &mut chest.slots[i], left, right);
                }
                return None;
            }
        }
        RightLayout::Craft { recipes, queue, .. } => {
            if left
                && ui.held.is_none()
                && let Some(i) = recipes.iter().position(|r| r.contains(m))
            {
                return Some(UiAction::Craft(i));
            }
            if right && let Some(i) = queue.iter().position(|r| r.contains(m)) {
                return Some(UiAction::CancelCraft(i));
            }
        }
    }

    let over_panel = layout.inv_panel.contains(m) || layout.right_panel.contains(m);
    if left
        && !over_panel
        && let Some(stack) = ui.held.take()
    {
        return Some(UiAction::Drop(stack));
    }
    None
}

fn slot_click(ui: &mut UiState, slot: &mut Option<Stack>, left: bool, right: bool) {
    if left {
        click_slot(ui, slot);
    } else if right {
        split_slot(ui, slot);
    }
}

/// Shift+clique: move a pilha inteira para o outro inventário (o que não couber fica).
fn transfer(slot: &mut Option<Stack>, to: &mut Inventory) {
    if let Some(stack) = slot.take() {
        *slot = to.insert_stack(stack);
    }
}

/// Clique esquerdo: pegar, soltar, juntar ou trocar pilhas.
fn click_slot(ui: &mut UiState, slot: &mut Option<Stack>) {
    match (ui.held.take(), slot.take()) {
        (None, s) => ui.held = s,
        (Some(h), None) => *slot = Some(h),
        (Some(mut h), Some(mut s)) if h.item == s.item && s.count < s.item.max_stack() => {
            let moved = (s.item.max_stack() - s.count).min(h.count);
            s.count += moved;
            h.count -= moved;
            *slot = Some(s);
            ui.held = (h.count > 0).then_some(h);
        }
        (Some(h), Some(s)) => {
            *slot = Some(h);
            ui.held = Some(s);
        }
    }
}

/// Clique direito: pega metade da pilha (sem nada no cursor) ou solta 1 unidade.
fn split_slot(ui: &mut UiState, slot: &mut Option<Stack>) {
    match (&mut ui.held, slot.as_mut()) {
        (None, Some(s)) if s.count > 1 => {
            let half = s.count / 2;
            s.count -= half;
            ui.held = Some(Stack { count: half, ..*s });
        }
        (Some(h), None) => {
            *slot = Some(Stack { count: 1, ..*h });
            h.count -= 1;
        }
        (Some(h), Some(s)) if h.item == s.item && s.count < s.item.max_stack() => {
            s.count += 1;
            h.count -= 1;
        }
        _ => {}
    }
    if ui.held.is_some_and(|h| h.count == 0) {
        ui.held = None;
    }
}

// ---------- Desenho ----------

pub fn draw(hud: &Hud) {
    if hud.dead {
        draw_death_screen();
        draw_messages(hud.messages);
        return;
    }
    if hud.ui.hide_help {
        shadowed("H: ajuda", 12.0, 22.0, 20.0, DIM);
    } else {
        draw_help();
    }
    if hud.hurt > 0.0 {
        let a = (hud.hurt / 0.35).min(1.0) * 0.25;
        draw_rectangle(0.0, 0.0, screen_width(), screen_height(), Color::new(0.8, 0.0, 0.0, a));
    }
    draw_clock(hud.time);
    draw_vitals(hud.vitals);
    if let Some(mount) = hud.mount {
        draw_mount_bar(mount);
    }
    draw_messages(hud.messages);
    if hud.ui.inventory_open {
        draw_panels(hud);
    } else {
        draw_queue_hud(hud.queue);
        draw_hotbar(hud.inventory, hud.selected, hud.mount.is_some());
    }
}

fn shadowed(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(text, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.55 * color.a));
    draw_text(text, x, y, size, color);
}

fn draw_help() {
    let lines = [
        "WASD: mover   Shift: correr   E / Clique: coletar ou beber   R / Botão dir.: comer",
        "F: interagir   Q: largar (Ctrl+Q: pilha)   Estrutura na mão: clique posiciona, X demole",
        "1-6 / Roda: hotbar   Tab / I: inventário   F5: salvar   H: esconder ajuda",
    ];
    for (i, line) in lines.iter().enumerate() {
        shadowed(line, 12.0, 22.0 + i as f32 * 20.0, 20.0, TEXT);
    }
}

fn draw_clock(time: f32) {
    let hour = daynight::hour(time);
    let label = format!(
        "Dia {}  {:02}:{:02}",
        daynight::day(time),
        hour as u32,
        ((hour.fract()) * 60.0) as u32 / 10 * 10
    );
    let size = 24.0;
    let w = measure_text(&label, None, size as u16, 1.0).width;
    let x = screen_width() - w - 16.0;
    shadowed(&label, x, 30.0, size, TEXT);
    let (ix, iy) = (x - 16.0, 22.0);
    if daynight::darkness(hour) < 0.4 {
        draw_circle(ix, iy, 8.0, Color::new(1.0, 0.85, 0.3, 1.0));
    } else {
        draw_circle(ix, iy, 8.0, Color::new(0.85, 0.88, 1.0, 1.0));
        draw_circle(ix + 4.0, iy - 3.0, 7.0, Color::new(0.10, 0.11, 0.18, 1.0));
    }
}

fn draw_vitals(v: &Vitals) {
    let bars = [
        ("Vida", v.health / MAX_HEALTH, Color::new(0.85, 0.25, 0.25, 1.0)),
        ("Comida", v.food / MAX_FOOD, Color::new(0.90, 0.60, 0.20, 1.0)),
        ("Água", v.water / MAX_WATER, Color::new(0.25, 0.60, 0.95, 1.0)),
        ("Stamina", v.stamina / MAX_STAMINA, Color::new(0.85, 0.85, 0.30, 1.0)),
    ];
    let (w, h, x) = (170.0, 12.0, 16.0);
    let blink = (get_time() * 6.0).sin() > 0.0;
    for (i, (label, frac, color)) in bars.into_iter().enumerate() {
        let y = screen_height() - 30.0 - (bars.len() - 1 - i) as f32 * 24.0;
        let low = frac < 0.2;
        let label_color = if low && blink { BAD } else { TEXT };
        shadowed(label, x, y + 11.0, 18.0, label_color);
        let bx = x + 64.0;
        draw_rectangle(bx, y, w, h, Color::new(0.0, 0.0, 0.0, 0.55));
        let fill = if label == "Stamina" && v.exhausted { DIM } else { color };
        draw_rectangle(bx, y, w * frac.clamp(0.0, 1.0), h, fill);
        draw_rectangle_lines(bx, y, w, h, 1.0, BORDER);
    }
}

fn draw_death_screen() {
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.25, 0.0, 0.0, 0.6),
    );
    let center = |text: &str, y: f32, size: f32, color: Color| {
        let w = measure_text(text, None, size as u16, 1.0).width;
        draw_text(text, (screen_width() - w) / 2.0, y, size, color);
    };
    let cy = screen_height() / 2.0;
    center("Você morreu", cy - 20.0, 64.0, Color::new(1.0, 0.85, 0.8, 1.0));
    center("Seus itens ficaram numa bolsa onde você caiu.", cy + 24.0, 24.0, TEXT);
    center("Pressione Enter ou clique para renascer na praia", cy + 60.0, 22.0, DIM);
}

fn draw_messages(messages: &[(String, f32)]) {
    for (i, (msg, ttl)) in messages.iter().rev().take(4).enumerate() {
        let alpha = ttl.min(1.0);
        let size = 24.0;
        let w = measure_text(msg, None, size as u16, 1.0).width;
        let x = (screen_width() - w) / 2.0;
        let y = screen_height() - 100.0 - i as f32 * 26.0;
        shadowed(msg, x, y, size, Color::new(1.0, 0.85, 0.6, alpha));
    }
}

/// Indicador compacto da fila de crafting quando o inventário está fechado.
fn draw_queue_hud(queue: &CraftQueue) {
    let (Some(job), Some(progress)) = (queue.jobs.first(), queue.current_progress()) else {
        return;
    };
    let (w, x, y) = (200.0, screen_width() - 216.0, 48.0);
    let icon = Rect::new(x, y, 32.0, 32.0);
    draw_rectangle(x - 6.0, y - 4.0, w + 12.0, 40.0, Color::new(0.0, 0.0, 0.0, 0.45));
    draw_item_icon(job.output, icon);
    let extra = queue.jobs.len() - 1;
    let label = if extra > 0 {
        format!("{} (+{extra})", job.output.name())
    } else {
        job.output.name().to_owned()
    };
    draw_text(&label, icon.right() + 6.0, y + 14.0, 18.0, TEXT);
    let bar = Rect::new(icon.right() + 6.0, y + 22.0, w - 38.0, 6.0);
    draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(1.0, 1.0, 1.0, 0.15));
    draw_rectangle(bar.x, bar.y, bar.w * progress, bar.h, ACCENT);
}

/// Vida da montaria, acima das barras do jogador.
fn draw_mount_bar(mount: &Creature) {
    let (x, w, h) = (16.0, 234.0, 10.0);
    let y = screen_height() - 30.0 - 4.0 * 24.0 - 14.0;
    let label = format!("{} Nv {}   F: desmontar", mount.species.name(), mount.level);
    shadowed(&label, x, y - 4.0, 18.0, OK);
    draw_rectangle(x, y, w, h, Color::new(0.0, 0.0, 0.0, 0.55));
    draw_rectangle(x, y, w * mount.health_frac(), h, Color::new(0.4, 0.8, 0.4, 1.0));
    draw_rectangle_lines(x, y, w, h, 1.0, BORDER);
}

fn draw_hotbar(inv: &Inventory, selected: usize, mounted: bool) {
    for (i, rect) in hotbar_slots().iter().enumerate() {
        draw_slot(*rect, inv.slots[i], i == selected);
        draw_text((i + 1).to_string(), rect.x + 4.0, rect.y + 14.0, 16.0, DIM);
    }
    if let Some(stack) = inv.slots[selected] {
        let name = match stack.item {
            _ if mounted => format!("{}  (montado: E / clique morde)", stack.item.name()),
            item if item.structure().is_some() => format!("{}  (clique para posicionar)", item.name()),
            ItemId::Slingshot => format!(
                "Estilingue  ({} pedras, clique mira no cursor)",
                inv.count(ItemId::Stone)
            ),
            ItemId::Narcotic => "Narcótico  (coloque no inventário do dino desacordado)".to_owned(),
            item => item.name().to_owned(),
        };
        let w = measure_text(&name, None, 20, 1.0).width;
        let y = hotbar_slots()[0].y - 8.0;
        shadowed(&name, (screen_width() - w) / 2.0, y, 20.0, TEXT);
    }
}

fn draw_panel(rect: Rect, title: &str) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL_BG);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 2.0, BORDER);
    draw_text(title, rect.x + PAD, rect.y + 28.0, 26.0, TEXT);
}

fn draw_panels(hud: &Hud) {
    let (ui, inv) = (hud.ui, hud.inventory);
    let storage = hud.container.and_then(|c| c.inventory());
    let layout = panel_layout(
        inv.slots.len(),
        hud.container.zip(storage).map(|(c, s)| (s.slots.len(), c.kind())),
        hud.queue.jobs.len(),
    );
    let m = mouse();

    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.35),
    );

    let p = layout.inv_panel;
    draw_panel(p, "Inventário");
    draw_text("hotbar", p.right() - 70.0, p.y + 28.0, 18.0, DIM);
    for (i, rect) in layout.slots.iter().enumerate() {
        draw_slot(*rect, inv.slots[i], false);
    }
    let mut tooltip: Option<String> = layout
        .slots
        .iter()
        .position(|r| r.contains(m))
        .and_then(|i| inv.slots[i])
        .map(|s| stack_tooltip(&s));

    let rp = layout.right_panel;
    match (&layout.right, hud.container.zip(storage)) {
        (RightLayout::Container { slots, info, buttons }, Some((view, chest))) => {
            draw_panel(rp, &view.title());
            for (i, rect) in slots.iter().enumerate() {
                draw_slot(*rect, chest.slots[i], false);
            }
            if let Some(s) = slots.iter().position(|r| r.contains(m)).and_then(|i| chest.slots[i]) {
                tooltip = Some(stack_tooltip(&s));
            }
            match view {
                ContainerView::Structure(fire) if view.kind() == PanelKind::Campfire => {
                    draw_fire_controls(fire, slots, buttons[0], m);
                }
                ContainerView::Creature(c) if c.is_tamed() => draw_tamed_info(c, *info, buttons, m),
                ContainerView::Creature(c) => draw_taming_info(c, *info),
                ContainerView::Structure(_) => {}
            }
            draw_text(
                "Shift+clique: transferir tudo",
                rp.x + PAD,
                rp.bottom() - 12.0,
                18.0,
                DIM,
            );
        }
        (RightLayout::Craft { recipes, detail, queue }, _) => {
            draw_panel(rp, "Crafting");
            let hovered = recipes.iter().position(|r| r.contains(m));
            for (i, (rect, recipe)) in recipes.iter().zip(RECIPES).enumerate() {
                let craftable = crafting::can_afford(inv, recipe) && (hud.station_ok)(recipe);
                draw_slot(*rect, None, false);
                draw_item_icon(recipe.output, *rect);
                draw_count(*rect, recipe.count);
                if craftable {
                    let thick = if hovered == Some(i) { 3.0 } else { 1.5 };
                    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, thick, ACCENT);
                } else {
                    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.0, 0.0, 0.0, 0.45));
                }
            }
            draw_recipe_detail(*detail, hovered.map(|i| &RECIPES[i]), hud);
            draw_queue(*detail, queue, hud.queue);
            if let Some(i) = queue.iter().position(|r| r.contains(m)) {
                tooltip = Some(format!("{} (botão dir.: cancelar)", hud.queue.jobs[i].output.name()));
            }
            draw_text(
                "Clique: fabricar   Botão dir. na fila: cancelar",
                rp.x + PAD,
                rp.bottom() - 30.0,
                16.0,
                DIM,
            );
            draw_text(
                "Solte um item fora dos painéis para largar",
                rp.x + PAD,
                rp.bottom() - 12.0,
                16.0,
                DIM,
            );
        }
        _ => {}
    }

    if let Some(held) = ui.held {
        let r = Rect::new(m.x - SLOT / 2.0, m.y - SLOT / 2.0, SLOT, SLOT);
        draw_item_icon(held.item, r);
        draw_count(r, held.count);
    } else if let Some(text) = tooltip {
        draw_tooltip(&text, m);
    }
}

/// Status da fogueira (combustível, cozimento) e botão acender/apagar.
fn draw_fire_controls(fire: &Structure, slots: &[Rect], button: Rect, m: Vec2) {
    let y = slots.last().map_or(button.y - 40.0, |r| r.bottom()) + 24.0;
    let x = button.x;
    let status = if fire.is_lit() {
        let secs = fire.fuel.ceil() as u32;
        format!("Acesa: {}:{:02} nesta lenha", secs / 60, secs % 60)
    } else {
        "Apagada. Coloque madeira ou palha.".to_owned()
    };
    draw_text(&status, x, y, 18.0, if fire.is_lit() { ACCENT } else { DIM });

    draw_button(button, if fire.active { "Apagar" } else { "Acender" }, true, m);

    if fire.is_lit() && fire.cook > 0.0 {
        let bar = Rect::new(button.right() + 16.0, button.y + 20.0, 200.0, 6.0);
        draw_text("Cozinhando carne", bar.x, button.y + 12.0, 16.0, TEXT);
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, Color::new(1.0, 1.0, 1.0, 0.15));
        draw_rectangle(bar.x, bar.y, bar.w * (fire.cook / COOK_SECS).min(1.0), bar.h, OK);
    }
}

fn draw_bar(x: f32, y: f32, w: f32, label: &str, frac: f32, color: Color) {
    draw_text(label, x, y, 16.0, TEXT);
    draw_rectangle(x, y + 4.0, w, 7.0, Color::new(1.0, 1.0, 1.0, 0.12));
    draw_rectangle(x, y + 4.0, w * frac.clamp(0.0, 1.0), 7.0, color);
}

fn draw_button(rect: Rect, label: &str, enabled: bool, m: Vec2) {
    let bg = match (enabled, rect.contains(m)) {
        (true, true) => ACCENT,
        (true, false) => Color::new(0.85, 0.65, 0.25, 1.0),
        _ => Color::new(1.0, 1.0, 1.0, 0.10),
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, bg);
    let w = measure_text(label, None, 20, 1.0).width;
    let fg = if enabled { Color::new(0.1, 0.08, 0.05, 1.0) } else { DIM };
    draw_text(label, rect.x + (rect.w - w) / 2.0, rect.y + 22.0, 20.0, fg);
}

/// Barras de torpor e domesticação de um dino desacordado + dica de comida.
fn draw_taming_info(c: &Creature, area: Rect) {
    let w = area.w;
    let torpor_label = format!("Torpor {:.0}/{:.0}", c.torpor, c.max_torpor());
    draw_bar(
        area.x,
        area.y + 14.0,
        w,
        &torpor_label,
        c.torpor_frac(),
        Color::new(0.65, 0.4, 0.95, 1.0),
    );
    match c.taming {
        Some(t) => {
            let label = format!(
                "Domesticação {:.0}%   Efetividade {:.0}%",
                t.progress * 100.0,
                t.effectiveness * 100.0
            );
            draw_bar(area.x, area.y + 44.0, w, &label, t.progress, OK);
            let foods: Vec<&str> = taming::preferred_foods(c.species.diet())
                .iter()
                .map(|f| f.name())
                .collect();
            let hint = format!("Come: {}", foods.join(" > "));
            draw_text(&hint, area.x, area.y + 80.0, 16.0, DIM);
            draw_text(
                "Narcótico o mantém dormindo. Não bata nele!",
                area.x,
                area.y + 98.0,
                16.0,
                DIM,
            );
        }
        None => {
            draw_text(
                "Esta espécie não pode ser domesticada.",
                area.x,
                area.y + 50.0,
                18.0,
                BAD,
            );
        }
    }
}

/// Vida, ordem atual e botões de um dino domesticado.
fn draw_tamed_info(c: &Creature, area: Rect, buttons: &[Rect], m: Vec2) {
    let label = format!("Vida {:.0}/{:.0}", c.health, c.max_health());
    draw_bar(
        area.x,
        area.y + 14.0,
        area.w,
        &label,
        c.health_frac(),
        Color::new(0.4, 0.8, 0.4, 1.0),
    );
    let (order, toggle) = match c.command {
        Some(Command::Stay) => ("Parado", "Seguir"),
        _ => ("Seguindo você", "Ficar"),
    };
    draw_text(format!("Ordem: {order}"), area.x, area.y + 44.0, 18.0, TEXT);
    let ride_hint = match c.species.saddle() {
        Some(_) if c.can_ride() => "Pronto para montar.".to_owned(),
        Some(saddle) => format!("Coloque uma {} aqui para montar.", saddle.name()),
        None => "Esta espécie não pode ser montada.".to_owned(),
    };
    draw_text(&ride_hint, area.x, area.y + 64.0, 16.0, DIM);
    draw_button(buttons[0], toggle, true, m);
    draw_button(buttons[1], "Montar", c.can_ride(), m);
}

fn stack_tooltip(stack: &Stack) -> String {
    match stack.item.max_durability() {
        Some(max) => format!("{} ({}/{max})", stack.item.name(), stack.durability),
        None => stack.item.name().to_owned(),
    }
}

fn draw_recipe_detail(area: Rect, recipe: Option<&Recipe>, hud: &Hud) {
    draw_rectangle(area.x, area.y, area.w, area.h, SLOT_BG);
    let Some(recipe) = recipe else {
        draw_text("Passe o mouse numa receita", area.x + 10.0, area.y + 26.0, 18.0, DIM);
        return;
    };
    draw_text(recipe.output.name(), area.x + 10.0, area.y + 24.0, 22.0, TEXT);
    let secs = format!("{:.0}s", recipe.secs.ceil());
    let w = measure_text(&secs, None, 18, 1.0).width;
    draw_text(&secs, area.right() - w - 10.0, area.y + 24.0, 18.0, DIM);

    let mut x = area.x + 10.0;
    for &(item, need) in recipe.inputs {
        let have = hud.inventory.count(item);
        let chip = Rect::new(x, area.y + 34.0, 22.0, 22.0);
        draw_item_icon(item, chip);
        let label = format!("{}/{}", have.min(need), need);
        let color = if have >= need { OK } else { BAD };
        draw_text(&label, chip.right() + 2.0, chip.y + 16.0, 18.0, color);
        x = chip.right() + measure_text(&label, None, 18, 1.0).width + 12.0;
    }

    if let Some(station) = recipe.station {
        let ok = (hud.station_ok)(recipe);
        let text = format!("Requer {} por perto", station.item().name());
        draw_text(
            &text,
            area.x + 10.0,
            area.bottom() - 12.0,
            18.0,
            if ok { OK } else { BAD },
        );
    }
}

fn draw_queue(detail: Rect, slots: &[Rect], queue: &CraftQueue) {
    let label = format!("Fila ({}/{MAX_QUEUE})", queue.jobs.len());
    draw_text(&label, detail.x, detail.bottom() + 12.0 + 16.0, 18.0, DIM);
    let progress = queue.current_progress().unwrap_or(0.0);
    for (i, (rect, job)) in slots.iter().zip(&queue.jobs).enumerate() {
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, SLOT_BG);
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, BORDER);
        draw_item_icon(job.output, *rect);
        if i == 0 {
            draw_rectangle(rect.x, rect.bottom() - 4.0, rect.w * progress, 4.0, ACCENT);
        }
    }
}

fn draw_tooltip(text: &str, at: Vec2) {
    let w = measure_text(text, None, 20, 1.0).width + 16.0;
    let x = (at.x + 14.0).min(screen_width() - w - 4.0);
    draw_rectangle(x, at.y + 10.0, w, 28.0, Color::new(0.0, 0.0, 0.0, 0.85));
    draw_text(text, x + 8.0, at.y + 30.0, 20.0, TEXT);
}

fn draw_slot(rect: Rect, stack: Option<Stack>, selected: bool) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, Color::new(0.08, 0.09, 0.10, 0.75));
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, SLOT_BG);
    let (thick, color) = if selected { (3.0, ACCENT) } else { (1.0, BORDER) };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, thick, color);
    if let Some(s) = stack {
        draw_item_icon(s.item, rect);
        draw_count(rect, s.count);
        if let Some(frac) = s.durability_frac() {
            let (bw, bh) = (rect.w - 8.0, 4.0);
            let (bx, by) = (rect.x + 4.0, rect.bottom() - 7.0);
            draw_rectangle(bx, by, bw, bh, Color::new(0.0, 0.0, 0.0, 0.6));
            let color = Color::new(1.0 - frac, 0.3 + frac * 0.55, 0.3, 1.0);
            draw_rectangle(bx, by, bw * frac, bh, color);
        }
    }
}

fn draw_count(rect: Rect, count: u32) {
    if count > 1 {
        let t = count.to_string();
        let w = measure_text(&t, None, 18, 1.0).width;
        shadowed(&t, rect.right() - w - 4.0, rect.bottom() - 4.0, 18.0, TEXT);
    }
}

/// Ícones provisórios desenhados com primitivas, normalizados para o retângulo do slot.
pub fn draw_item_icon(item: ItemId, rect: Rect) {
    let c = rect.center();
    let u = rect.w / 52.0; // escala relativa ao slot padrão
    let p = |dx: f32, dy: f32| vec2(c.x + dx * u, c.y + dy * u);
    let wood = Color::from_rgba(150, 104, 60, 255);
    let wood_light = Color::from_rgba(214, 170, 112, 255);
    let wood_dark = Color::from_rgba(97, 64, 33, 255);
    let stone = Color::from_rgba(150, 150, 158, 255);
    let stone_dark = Color::from_rgba(104, 104, 112, 255);
    let line = |a: Vec2, b: Vec2, t: f32, col| draw_line(a.x, a.y, b.x, b.y, t * u, col);
    let rect_at = |dx: f32, dy: f32, w: f32, h: f32, col| draw_rectangle(c.x + dx * u, c.y + dy * u, w * u, h * u, col);

    match item {
        ItemId::Wood => {
            for dy in [-7.0, 7.0] {
                let (a, b) = (p(-14.0, dy + 4.0), p(14.0, dy - 4.0));
                line(a, b, 10.0, wood);
                draw_circle(b.x, b.y, 5.0 * u, wood_light);
            }
        }
        ItemId::Thatch => {
            let col = Color::from_rgba(200, 186, 96, 255);
            for i in -3..=3 {
                let dx = i as f32 * 3.5;
                line(p(dx * 0.4, 14.0), p(dx, -14.0), 2.5, col);
            }
            line(p(-6.0, 4.0), p(6.0, 4.0), 3.0, wood);
        }
        ItemId::Stone => {
            let a = p(0.0, 2.0);
            draw_ellipse(a.x, a.y, 15.0 * u, 11.0 * u, 0.0, stone_dark);
            let b = p(-3.0, -1.0);
            draw_ellipse(b.x, b.y, 10.0 * u, 7.0 * u, 0.0, stone);
        }
        ItemId::Flint => {
            draw_triangle(
                p(-12.0, 10.0),
                p(12.0, 8.0),
                p(2.0, -14.0),
                Color::from_rgba(60, 62, 72, 255),
            );
            draw_triangle(
                p(-4.0, 4.0),
                p(4.0, 4.0),
                p(1.0, -8.0),
                Color::from_rgba(96, 100, 116, 255),
            );
        }
        ItemId::Berry => {
            let red = Color::from_rgba(196, 40, 72, 255);
            let hi = Color::from_rgba(240, 130, 150, 255);
            for (dx, dy) in [(-6.0, 4.0), (6.0, 4.0), (0.0, -6.0)] {
                let b = p(dx, dy);
                draw_circle(b.x, b.y, 7.0 * u, red);
                draw_circle(b.x - 2.0 * u, b.y - 2.0 * u, 2.0 * u, hi);
            }
            line(p(0.0, -12.0), p(4.0, -18.0), 2.0, Color::from_rgba(70, 120, 50, 255));
        }
        ItemId::RawMeat | ItemId::CookedMeat => {
            let (meat, fat) = if item == ItemId::RawMeat {
                (Color::from_rgba(200, 70, 80, 255), Color::from_rgba(240, 190, 190, 255))
            } else {
                (Color::from_rgba(150, 82, 40, 255), Color::from_rgba(210, 150, 90, 255))
            };
            let a = p(-3.0, 2.0);
            draw_ellipse(a.x, a.y, 13.0 * u, 10.0 * u, 0.0, meat);
            draw_ellipse(a.x - 3.0 * u, a.y - 3.0 * u, 6.0 * u, 3.0 * u, 0.0, fat);
            line(p(8.0, -4.0), p(16.0, -12.0), 4.0, Color::from_rgba(235, 228, 210, 255));
            let b = p(17.0, -13.0);
            draw_circle(b.x, b.y, 3.0 * u, Color::from_rgba(235, 228, 210, 255));
        }
        ItemId::Hide => {
            let col = Color::from_rgba(160, 120, 80, 255);
            draw_ellipse(c.x, c.y, 15.0 * u, 11.0 * u, 0.3, col);
            for (dx, dy) in [(-13.0, -9.0), (13.0, -9.0), (-12.0, 10.0), (12.0, 10.0)] {
                let q = p(dx, dy);
                draw_circle(q.x, q.y, 4.0 * u, col);
            }
            draw_ellipse(c.x, c.y, 7.0 * u, 5.0 * u, 0.3, Color::from_rgba(130, 94, 60, 255));
        }
        ItemId::StonePickaxe => {
            line(p(-12.0, 14.0), p(8.0, -10.0), 4.0, wood);
            line(p(-4.0, -16.0), p(16.0, 0.0), 6.0, stone);
        }
        ItemId::StoneHatchet => {
            line(p(-12.0, 14.0), p(8.0, -10.0), 4.0, wood);
            let h = p(8.0, -10.0);
            draw_circle(h.x, h.y, 8.0 * u, stone);
            draw_circle(
                h.x - 2.0 * u,
                h.y - 2.0 * u,
                3.0 * u,
                Color::from_rgba(186, 186, 194, 255),
            );
        }
        ItemId::WoodenSpear => {
            line(p(-16.0, 16.0), p(10.0, -10.0), 3.5, wood);
            draw_triangle(
                p(18.0, -18.0),
                p(6.0, -10.0),
                p(10.0, -6.0),
                Color::from_rgba(60, 62, 72, 255),
            );
        }
        ItemId::WoodenClub => {
            line(p(-14.0, 16.0), p(6.0, -8.0), 4.0, wood);
            let head = p(8.0, -11.0);
            draw_circle(head.x, head.y, 8.0 * u, wood);
            draw_circle(head.x - 2.0 * u, head.y - 2.0 * u, 3.0 * u, wood_light);
        }
        ItemId::Slingshot => {
            line(p(0.0, 16.0), p(0.0, 0.0), 4.0, wood);
            line(p(0.0, 0.0), p(-9.0, -14.0), 4.0, wood);
            line(p(0.0, 0.0), p(9.0, -14.0), 4.0, wood);
            line(p(-9.0, -14.0), p(0.0, -6.0), 1.5, Color::from_rgba(120, 84, 56, 255));
            line(p(9.0, -14.0), p(0.0, -6.0), 1.5, Color::from_rgba(120, 84, 56, 255));
            let s = p(0.0, -6.0);
            draw_circle(s.x, s.y, 3.0 * u, stone);
        }
        ItemId::Narcotic => {
            let glass = Color::from_rgba(200, 210, 220, 200);
            rect_at(-8.0, -6.0, 16.0, 20.0, glass);
            rect_at(-6.0, 0.0, 12.0, 12.0, Color::from_rgba(120, 60, 150, 255));
            rect_at(-4.0, -12.0, 8.0, 6.0, wood_dark);
        }
        ItemId::RaptorSaddle => {
            let leather = Color::from_rgba(120, 76, 40, 255);
            draw_ellipse(c.x, c.y, 16.0 * u, 9.0 * u, 0.0, leather);
            rect_at(-12.0, -8.0, 6.0, 8.0, Color::from_rgba(150, 100, 56, 255));
            line(p(-4.0, 6.0), p(-6.0, 16.0), 2.0, wood_dark);
            line(p(6.0, 6.0), p(8.0, 16.0), 2.0, wood_dark);
        }
        ItemId::Campfire => {
            line(p(-14.0, 12.0), p(14.0, 6.0), 6.0, wood);
            line(p(-14.0, 6.0), p(14.0, 12.0), 6.0, wood);
            draw_triangle(
                p(-9.0, 6.0),
                p(9.0, 6.0),
                p(0.0, -16.0),
                Color::from_rgba(240, 120, 40, 255),
            );
            draw_triangle(
                p(-4.0, 6.0),
                p(4.0, 6.0),
                p(0.0, -6.0),
                Color::from_rgba(255, 210, 90, 255),
            );
        }
        ItemId::Workbench => {
            rect_at(-16.0, -6.0, 32.0, 6.0, wood_light);
            rect_at(-16.0, 0.0, 32.0, 3.0, wood);
            rect_at(-13.0, 3.0, 4.0, 12.0, wood_dark);
            rect_at(9.0, 3.0, 4.0, 12.0, wood_dark);
            rect_at(-6.0, -11.0, 7.0, 5.0, stone);
        }
        ItemId::StorageBox => {
            rect_at(-15.0, -6.0, 30.0, 18.0, wood);
            rect_at(-15.0, -13.0, 30.0, 7.0, wood_light);
            rect_at(-11.0, -13.0, 3.0, 25.0, wood_dark);
            rect_at(8.0, -13.0, 3.0, 25.0, wood_dark);
            rect_at(-2.0, -6.0, 4.0, 5.0, Color::from_rgba(200, 190, 150, 255));
        }
        ItemId::WoodFoundation => {
            rect_at(-15.0, -15.0, 30.0, 30.0, wood);
            for i in 1..4 {
                let y = -15.0 + i as f32 * 7.5;
                line(p(-15.0, y), p(15.0, y), 1.5, wood_dark);
            }
        }
        ItemId::WoodWall => {
            rect_at(-15.0, -16.0, 30.0, 10.0, wood_light);
            rect_at(-15.0, -6.0, 30.0, 20.0, wood);
            for i in 1..4 {
                let x = -15.0 + i as f32 * 7.5;
                line(p(x, -6.0), p(x, 14.0), 1.5, wood_dark);
            }
        }
        ItemId::WoodDoor => {
            rect_at(-11.0, -16.0, 22.0, 32.0, wood);
            draw_rectangle_lines(c.x - 8.0 * u, c.y - 13.0 * u, 16.0 * u, 26.0 * u, 1.5 * u, wood_dark);
            let k = p(6.0, 1.0);
            draw_circle(k.x, k.y, 2.0 * u, Color::from_rgba(220, 190, 100, 255));
        }
    }
}
