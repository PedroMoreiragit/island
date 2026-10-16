//! Inventário em slots com empilhamento. Os primeiros `HOTBAR_SIZE` slots formam a hotbar.

use crate::items::ItemId;

pub const INVENTORY_SIZE: usize = 30;
pub const HOTBAR_SIZE: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: ItemId,
    pub count: u32,
}

#[derive(Clone, Debug)]
pub struct Inventory {
    pub slots: Vec<Option<Stack>>,
}

impl Inventory {
    pub fn new(size: usize) -> Self {
        Self {
            slots: vec![None; size],
        }
    }

    /// Adiciona itens completando pilhas existentes antes de ocupar slots vazios.
    /// Retorna a quantidade que não coube.
    pub fn add(&mut self, item: ItemId, mut count: u32) -> u32 {
        let max = item.max_stack();
        for stack in self.slots.iter_mut().flatten() {
            if count == 0 {
                break;
            }
            if stack.item == item && stack.count < max {
                let n = (max - stack.count).min(count);
                stack.count += n;
                count -= n;
            }
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            if count == 0 {
                break;
            }
            let n = max.min(count);
            *slot = Some(Stack { item, count: n });
            count -= n;
        }
        count
    }

    pub fn count(&self, item: ItemId) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.item == item)
            .map(|s| s.count)
            .sum()
    }

    /// Remove `count` unidades (tudo ou nada), consumindo do fim para preservar a hotbar.
    pub fn remove(&mut self, item: ItemId, mut count: u32) -> bool {
        if self.count(item) < count {
            return false;
        }
        for slot in self.slots.iter_mut().rev() {
            if count == 0 {
                break;
            }
            let Some(stack) = slot.as_mut() else { continue };
            if stack.item != item {
                continue;
            }
            let n = stack.count.min(count);
            stack.count -= n;
            count -= n;
            if stack.count == 0 {
                *slot = None;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_stacks_then_overflows() {
        let mut inv = Inventory::new(2);
        assert_eq!(inv.add(ItemId::Wood, 150), 0);
        assert_eq!(inv.slots[0].unwrap().count, 100);
        assert_eq!(inv.slots[1].unwrap().count, 50);
        assert_eq!(inv.add(ItemId::Wood, 60), 10);
        assert_eq!(inv.count(ItemId::Wood), 200);
    }

    #[test]
    fn tools_do_not_stack() {
        let mut inv = Inventory::new(3);
        assert_eq!(inv.add(ItemId::StonePickaxe, 2), 0);
        assert!(inv.slots[0].is_some() && inv.slots[1].is_some() && inv.slots[2].is_none());
    }

    #[test]
    fn remove_is_all_or_nothing() {
        let mut inv = Inventory::new(4);
        inv.add(ItemId::Stone, 5);
        assert!(!inv.remove(ItemId::Stone, 6));
        assert_eq!(inv.count(ItemId::Stone), 5);
        assert!(inv.remove(ItemId::Stone, 5));
        assert!(inv.slots.iter().all(Option::is_none));
    }
}
