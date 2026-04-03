use rand::seq::SliceRandom;
use rand::thread_rng;

use crate::app::RepeatMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: Option<u64>,
}

#[derive(Debug, Default)]
pub struct PlaybackQueue {
    items: Vec<QueueItem>,
    index: Option<usize>,
    shuffle: bool,
    repeat: RepeatMode,
    history: Vec<usize>,
}

impl PlaybackQueue {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            index: None,
            shuffle: false,
            repeat: RepeatMode::None,
            history: Vec::new(),
        }
    }

    pub fn set_items(&mut self, items: Vec<QueueItem>) {
        self.items = items;
        self.index = None;
        self.history.clear();
    }

    pub fn enqueue(&mut self, item: QueueItem) {
        self.items.push(item);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn current(&self) -> Option<&QueueItem> {
        self.index.and_then(|idx| self.items.get(idx))
    }

    pub fn current_index(&self) -> Option<usize> {
        self.index
    }

    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    /// Move the item at `idx` one position up. Returns the new index.
    pub fn move_up(&mut self, idx: usize) -> usize {
        if idx > 0 && idx < self.items.len() {
            self.items.swap(idx, idx - 1);
            // Adjust current playback index if affected
            if let Some(ref mut ci) = self.index {
                if *ci == idx {
                    *ci = idx - 1;
                } else if *ci == idx - 1 {
                    *ci = idx;
                }
            }
            idx - 1
        } else {
            idx
        }
    }

    /// Move the item at `idx` one position down. Returns the new index.
    pub fn move_down(&mut self, idx: usize) -> usize {
        if idx + 1 < self.items.len() {
            self.items.swap(idx, idx + 1);
            // Adjust current playback index if affected
            if let Some(ref mut ci) = self.index {
                if *ci == idx {
                    *ci = idx + 1;
                } else if *ci == idx + 1 {
                    *ci = idx;
                }
            }
            idx + 1
        } else {
            idx
        }
    }

    /// Remove the item at `idx`. Returns the removed item if valid.
    pub fn remove(&mut self, idx: usize) -> Option<QueueItem> {
        if idx >= self.items.len() {
            return None;
        }
        let item = self.items.remove(idx);
        // Adjust current index
        if let Some(ref mut ci) = self.index {
            if *ci == idx {
                // Current song was removed; keep index pointing at same position
                if self.items.is_empty() {
                    self.index = None;
                } else if *ci >= self.items.len() {
                    *ci = self.items.len() - 1;
                }
            } else if *ci > idx {
                *ci -= 1;
            }
        }
        self.history.retain(|&h| h != idx);
        for h in &mut self.history {
            if *h > idx {
                *h -= 1;
            }
        }
        Some(item)
    }

    pub fn next(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }

        if let (RepeatMode::One, Some(idx)) = (self.repeat, self.index) {
            return self.items.get(idx);
        }

        let next_index = if self.shuffle {
            let mut rng = thread_rng();
            let mut indices: Vec<usize> = (0..self.items.len()).collect();
            if let Some(current) = self.index {
                indices.retain(|i| *i != current);
            }
            indices.shuffle(&mut rng);
            indices.first().copied().or(self.index)
        } else {
            match self.index {
                Some(idx) => Some(idx + 1),
                None => Some(0),
            }
        };

        let next_index = match next_index {
            Some(idx) if idx < self.items.len() => Some(idx),
            _ => match self.repeat {
                RepeatMode::All => Some(0),
                _ => None,
            },
        };

        if let Some(current) = self.index {
            self.history.push(current);
        }

        self.index = next_index;
        self.current()
    }

    pub fn previous(&mut self) -> Option<&QueueItem> {
        if self.items.is_empty() {
            return None;
        }

        if let Some(prev_idx) = self.history.pop() {
            self.index = Some(prev_idx);
            return self.current();
        }

        let prev_index = match self.index {
            Some(idx) if idx > 0 => Some(idx - 1),
            Some(_) => match self.repeat {
                RepeatMode::All => Some(self.items.len().saturating_sub(1)),
                _ => None,
            },
            None => Some(0),
        };

        self.index = prev_index;
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_item(id: &str) -> QueueItem {
        QueueItem {
            id: id.to_string(),
            title: format!("Song {id}"),
            url: format!("https://example.com/{id}"),
            duration: Some(120),
        }
    }

    #[test]
    fn test_next_linear() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));

        assert_eq!(queue.next().unwrap().id, "a");
        assert_eq!(queue.next().unwrap().id, "b");
        assert!(queue.next().is_none());
    }

    #[test]
    fn test_repeat_all() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.set_repeat(RepeatMode::All);

        assert_eq!(queue.next().unwrap().id, "a");
        assert_eq!(queue.next().unwrap().id, "b");
        assert_eq!(queue.next().unwrap().id, "a");
    }

    #[test]
    fn test_previous_uses_history() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));

        queue.next();
        queue.next();
        assert_eq!(queue.previous().unwrap().id, "a");
    }

    #[test]
    fn test_move_up() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.enqueue(make_item("c"));

        let new_idx = queue.move_up(1);
        assert_eq!(new_idx, 0);
        assert_eq!(queue.items()[0].id, "b");
        assert_eq!(queue.items()[1].id, "a");
        assert_eq!(queue.items()[2].id, "c");
    }

    #[test]
    fn test_move_up_first_item_stays() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));

        let new_idx = queue.move_up(0);
        assert_eq!(new_idx, 0);
        assert_eq!(queue.items()[0].id, "a");
    }

    #[test]
    fn test_move_down() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.enqueue(make_item("c"));

        let new_idx = queue.move_down(0);
        assert_eq!(new_idx, 1);
        assert_eq!(queue.items()[0].id, "b");
        assert_eq!(queue.items()[1].id, "a");
        assert_eq!(queue.items()[2].id, "c");
    }

    #[test]
    fn test_move_down_last_item_stays() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));

        let new_idx = queue.move_down(1);
        assert_eq!(new_idx, 1);
        assert_eq!(queue.items()[1].id, "b");
    }

    #[test]
    fn test_remove() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.enqueue(make_item("c"));

        let removed = queue.remove(1).unwrap();
        assert_eq!(removed.id, "b");
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.items()[0].id, "a");
        assert_eq!(queue.items()[1].id, "c");
    }

    #[test]
    fn test_remove_adjusts_current_index() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.enqueue(make_item("c"));

        // Advance to item "b" (index 1)
        queue.next(); // index 0
        queue.next(); // index 1
        assert_eq!(queue.current().unwrap().id, "b");

        // Remove item before current
        queue.remove(0);
        // Current index should shift down
        assert_eq!(queue.current().unwrap().id, "b");
    }

    #[test]
    fn test_move_up_adjusts_current_index() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));
        queue.enqueue(make_item("c"));

        queue.next(); // now at index 0 = "a"
        queue.next(); // now at index 1 = "b"
        assert_eq!(queue.current().unwrap().id, "b");

        // Move "b" (index 1) up to index 0
        queue.move_up(1);
        // Current index should follow to 0
        assert_eq!(queue.current().unwrap().id, "b");
    }

    #[test]
    fn test_items_and_current_index() {
        let mut queue = PlaybackQueue::new();
        queue.enqueue(make_item("a"));
        queue.enqueue(make_item("b"));

        assert_eq!(queue.current_index(), None);
        assert_eq!(queue.items().len(), 2);

        queue.next();
        assert_eq!(queue.current_index(), Some(0));
    }
}
