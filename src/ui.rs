//! HUD (hotbar, mensagens, ajuda) e painel de inventário + crafting em modo imediato.
//! Layout é recalculado a cada frame pelas mesmas funções usadas no input e no desenho.

use macroquad::prelude::*;

use crate::crafting::{self, RECIPES};
use crate::inventory::{HOTBAR_SIZE, Inventory, Stack};
use crate::items::ItemId;
use crate::vitals::{MAX_FOOD, MAX_HEALTH, MAX_STAMINA, MAX_WATER, Vitals};

const SLOT: f32 = 52.0;
const GAP: f32 = 6.0;
const PAD: f32 = 16.0;
const HEADER: f32 = 36.0;
const COLS: usize = 6;
const CRAFT_W: f32 = 400.0;
const ROW_H: f32 = 64.0;

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
}

pub enum UiAction {
    Craft(usize),
    SelectHotbar(usize),
    /// Pilha solta fora dos painéis: vai para o chão.
    Drop(Stack),
}

/// Tudo o que o HUD precisa ler do jogo.
pub struct Hud<'a> {
    pub ui: &'a UiState,
    pub inventory: &'a Inventory,
    pub selected: usize,
    pub messages: &'a [(String, f32)],
    pub vitals: &'a Vitals,
    pub dead: bool,
}

struct PanelLayout {
    inv_panel: Rect,
    slots: Vec<Rect>,
    craft_panel: Rect,
    craft_rows: Vec<Rect>,
}

fn panel_layout(n_slots: usize) -> PanelLayout {
    let rows = n_slots.div_ceil(COLS);
    let grid_w = COLS as f32 * (SLOT + GAP) - GAP;
    let grid_h = rows as f32 * (SLOT + GAP) - GAP;
    let inv_w = grid_w + PAD * 2.0;
    let inv_h = grid_h + PAD * 2.0 + HEADER;
    let craft_h = (HEADER + PAD * 2.0 + 44.0 + RECIPES.len() as f32 * (ROW_H + GAP)).max(inv_h);
    let total_w = inv_w + 16.0 + CRAFT_W;
    let x0 = ((screen_width() - total_w) / 2.0).max(8.0);
    let y0 = ((screen_height() - craft_h) / 2.0 - 40.0).max(8.0);

    let slots = (0..n_slots)
        .map(|i| {
            let (c, r) = ((i % COLS) as f32, (i / COLS) as f32);
            // Primeira linha (hotbar) levemente separada do resto.
            let extra = if i >= COLS { 8.0 } else { 0.0 };
            Rect::new(
                x0 + PAD + c * (SLOT + GAP),
                y0 + HEADER + PAD + r * (SLOT + GAP) + extra,
                SLOT,
                SLOT,
            )
        })
        .collect();

    let cx = x0 + inv_w + 16.0;
    let craft_rows: Vec<Rect> = (0..RECIPES.len())
        .map(|i| {
            Rect::new(
                cx + PAD,
                y0 + HEADER + PAD + i as f32 * (ROW_H + GAP),
                CRAFT_W - PAD * 2.0,
                ROW_H,
            )
        })
        .collect();

    PanelLayout {
        inv_panel: Rect::new(x0, y0, inv_w, inv_h + 8.0),
        slots,
        craft_panel: Rect::new(cx, y0, CRAFT_W, craft_h),
        craft_rows,
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

/// O cursor está sobre algum elemento de UI (para não coletar ao clicar na hotbar)?
pub fn pointer_over_ui(ui: &UiState) -> bool {
    let m = mouse();
    ui.inventory_open || hotbar_slots().iter().any(|r| r.contains(m))
}

/// Coloca de volta no inventário a pilha que estava no cursor.
pub fn return_held(ui: &mut UiState, inv: &mut Inventory) {
    if let Some(stack) = ui.held.take() {
        let leftover = inv.add(stack.item, stack.count);
        debug_assert_eq!(leftover, 0, "a pilha veio do próprio inventário");
    }
}

pub fn handle_input(ui: &mut UiState, inv: &mut Inventory) -> Option<UiAction> {
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

    let layout = panel_layout(inv.slots.len());
    let left = is_mouse_button_pressed(MouseButton::Left);
    let right = is_mouse_button_pressed(MouseButton::Right);

    if let Some(i) = layout.slots.iter().position(|r| r.contains(m)) {
        if left {
            click_slot(ui, &mut inv.slots[i]);
        } else if right {
            split_slot(ui, &mut inv.slots[i]);
        }
        return None;
    }

    let over_panel = layout.inv_panel.contains(m) || layout.craft_panel.contains(m);
    if left
        && !over_panel
        && let Some(stack) = ui.held.take()
    {
        return Some(UiAction::Drop(stack));
    }

    if left && ui.held.is_none() {
        let clicked = layout.craft_rows.iter().position(|r| r.contains(m));
        if let Some(i) = clicked.filter(|&i| crafting::can_afford(inv, &RECIPES[i])) {
            return Some(UiAction::Craft(i));
        }
    }
    None
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
    draw_help();
    draw_vitals(hud.vitals);
    draw_messages(hud.messages);
    if hud.ui.inventory_open {
        draw_panels(hud.ui, hud.inventory);
    } else {
        draw_hotbar(hud.inventory, hud.selected);
    }
}

fn draw_help() {
    let lines = [
        "WASD: mover   Shift: correr   E / Clique: coletar ou beber (na água)",
        "R / Botão dir.: comer   Q: largar (Ctrl+Q: pilha)   F: pegar",
        "1-6 / Roda: hotbar   Tab / I: inventário   F5: salvar",
    ];
    for (i, line) in lines.iter().enumerate() {
        let y = 22.0 + i as f32 * 20.0;
        draw_text(line, 13.0, y + 1.0, 20.0, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_text(line, 12.0, y, 20.0, TEXT);
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
        draw_text(label, x, y + 11.0, 18.0, label_color);
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
        draw_text(msg, x + 1.0, y + 1.0, size, Color::new(0.0, 0.0, 0.0, 0.6 * alpha));
        draw_text(msg, x, y, size, Color::new(1.0, 0.85, 0.6, alpha));
    }
}

fn draw_hotbar(inv: &Inventory, selected: usize) {
    for (i, rect) in hotbar_slots().iter().enumerate() {
        draw_slot(*rect, inv.slots[i], i == selected);
        draw_text((i + 1).to_string(), rect.x + 4.0, rect.y + 14.0, 16.0, DIM);
    }
    if let Some(stack) = inv.slots[selected] {
        let name = stack.item.name();
        let w = measure_text(name, None, 20, 1.0).width;
        let y = hotbar_slots()[0].y - 8.0;
        draw_text(name, (screen_width() - w) / 2.0, y, 20.0, TEXT);
    }
}

fn draw_panels(ui: &UiState, inv: &Inventory) {
    let layout = panel_layout(inv.slots.len());
    let m = mouse();

    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.0, 0.0, 0.0, 0.35),
    );

    // Inventário
    let p = layout.inv_panel;
    draw_rectangle(p.x, p.y, p.w, p.h, PANEL_BG);
    draw_rectangle_lines(p.x, p.y, p.w, p.h, 2.0, BORDER);
    draw_text("Inventário", p.x + PAD, p.y + 28.0, 26.0, TEXT);
    draw_text("hotbar", p.right() - 70.0, p.y + 28.0, 18.0, DIM);
    for (i, rect) in layout.slots.iter().enumerate() {
        draw_slot(*rect, inv.slots[i], false);
    }

    // Crafting
    let c = layout.craft_panel;
    draw_rectangle(c.x, c.y, c.w, c.h, PANEL_BG);
    draw_rectangle_lines(c.x, c.y, c.w, c.h, 2.0, BORDER);
    draw_text("Crafting", c.x + PAD, c.y + 28.0, 26.0, TEXT);
    let mut tooltip = None;
    for (i, recipe) in RECIPES.iter().enumerate() {
        let row = layout.craft_rows[i];
        // A linha inteira é o botão de fabricar.
        let enabled = crafting::can_afford(inv, recipe) && ui.held.is_none();
        let hover = enabled && row.contains(m);
        draw_rectangle(
            row.x,
            row.y,
            row.w,
            row.h,
            if hover {
                Color::new(1.0, 0.8, 0.35, 0.18)
            } else {
                SLOT_BG
            },
        );
        if enabled {
            draw_rectangle_lines(row.x, row.y, row.w, row.h, if hover { 2.0 } else { 1.0 }, ACCENT);
        }

        let icon = Rect::new(row.x + 6.0, row.y + 6.0, ROW_H - 12.0, ROW_H - 12.0);
        draw_item_icon(recipe.output, icon);
        draw_text(
            recipe.output.name(),
            icon.right() + 10.0,
            row.y + 22.0,
            22.0,
            if enabled { TEXT } else { DIM },
        );

        // Requisitos como chips: ícone + possui/precisa.
        let mut x = icon.right() + 8.0;
        for &(item, need) in recipe.inputs {
            let have = inv.count(item);
            let chip = Rect::new(x, row.y + 32.0, 22.0, 22.0);
            draw_item_icon(item, chip);
            let label = format!("{}/{}", have.min(need), need);
            draw_text(
                &label,
                chip.right() + 2.0,
                chip.y + 16.0,
                18.0,
                if have >= need { OK } else { BAD },
            );
            x = chip.right() + measure_text(&label, None, 18, 1.0).width + 12.0;
        }
        if row.contains(m) && ui.held.is_none() {
            let names: Vec<_> = recipe
                .inputs
                .iter()
                .map(|&(it, n)| format!("{n} {}", it.name()))
                .collect();
            tooltip = Some(names.join(", "));
        }
    }
    draw_text(
        "Clique numa receita para fabricar.",
        c.x + PAD,
        c.bottom() - 32.0,
        18.0,
        DIM,
    );
    draw_text(
        "Solte um item fora dos painéis para largar.",
        c.x + PAD,
        c.bottom() - 12.0,
        18.0,
        DIM,
    );

    // Tooltip e pilha no cursor
    if let Some(held) = ui.held {
        let r = Rect::new(m.x - SLOT / 2.0, m.y - SLOT / 2.0, SLOT, SLOT);
        draw_item_icon(held.item, r);
        draw_count(r, held.count);
    } else if let Some(stack) = layout
        .slots
        .iter()
        .position(|r| r.contains(m))
        .and_then(|i| inv.slots[i])
    {
        let text = match stack.item.max_durability() {
            Some(max) => format!("{} ({}/{max})", stack.item.name(), stack.durability),
            None => stack.item.name().to_owned(),
        };
        draw_tooltip(&text, m);
    } else if let Some(text) = tooltip {
        draw_tooltip(&text, m);
    }
}

fn draw_tooltip(text: &str, at: Vec2) {
    let w = measure_text(text, None, 20, 1.0).width + 16.0;
    draw_rectangle(at.x + 14.0, at.y + 10.0, w, 28.0, Color::new(0.0, 0.0, 0.0, 0.85));
    draw_text(text, at.x + 22.0, at.y + 30.0, 20.0, TEXT);
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
        draw_text(
            &t,
            rect.right() - w - 3.0,
            rect.bottom() - 3.0,
            18.0,
            Color::new(0.0, 0.0, 0.0, 0.8),
        );
        draw_text(&t, rect.right() - w - 4.0, rect.bottom() - 4.0, 18.0, TEXT);
    }
}

/// Ícones provisórios desenhados com primitivas, normalizados para o retângulo do slot.
pub fn draw_item_icon(item: ItemId, rect: Rect) {
    let c = rect.center();
    let u = rect.w / 52.0; // escala relativa ao slot padrão
    let p = |dx: f32, dy: f32| vec2(c.x + dx * u, c.y + dy * u);
    let wood = Color::from_rgba(150, 104, 60, 255);
    let wood_light = Color::from_rgba(214, 170, 112, 255);
    let stone = Color::from_rgba(150, 150, 158, 255);
    let stone_dark = Color::from_rgba(104, 104, 112, 255);
    let line = |a: Vec2, b: Vec2, t: f32, col| draw_line(a.x, a.y, b.x, b.y, t * u, col);

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
    }
}
