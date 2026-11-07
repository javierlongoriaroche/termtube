use rand::seq::SliceRandom;
use rand::Rng;

use crate::ytm::Track;

/// Remaining upcoming tracks below which radio refills the queue.
const RADIO_THRESHOLD: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    #[default]
    Off,
    All,
    One,
}

impl Repeat {
    pub fn cycle(self) -> Self {
        match self {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        }
    }
}

/// Play queue: an ordered list of tracks and the position being played.
#[derive(Debug, Clone, Default)]
pub struct Queue {
    tracks: Vec<Track>,
    current: Option<usize>,
    repeat: Repeat,
    /// Order before shuffling, restored when shuffle is turned off.
    unshuffled: Option<Vec<Track>>,
}

impl Queue {
    /// Replaces the queue and starts at `start`.
    pub fn set(&mut self, tracks: Vec<Track>, start: usize) {
        self.current = (!tracks.is_empty()).then(|| start.min(tracks.len() - 1));
        self.tracks = tracks;
        self.unshuffled = None;
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    pub fn current(&self) -> Option<&Track> {
        self.current.and_then(|i| self.tracks.get(i))
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    pub fn is_shuffled(&self) -> bool {
        self.unshuffled.is_some()
    }

    /// Track that `advance(true)` would move to, without moving.
    pub fn peek_next(&self) -> Option<&Track> {
        self.next_index(true).and_then(|i| self.tracks.get(i))
    }

    /// Moves to the next track; `auto` is true when the previous one ended by itself.
    pub fn advance(&mut self, auto: bool) -> Option<&Track> {
        let next = self.next_index(auto)?;
        self.current = Some(next);
        self.current()
    }

    fn next_index(&self, auto: bool) -> Option<usize> {
        let cur = self.current?;
        if auto && self.repeat == Repeat::One {
            return Some(cur);
        }
        if cur + 1 < self.tracks.len() {
            Some(cur + 1)
        } else if self.repeat != Repeat::Off {
            Some(0)
        } else {
            None
        }
    }

    /// Moves to the previous track (stays on the first one).
    pub fn back(&mut self) -> Option<&Track> {
        let cur = self.current?;
        self.current = Some(cur.saturating_sub(1));
        self.current()
    }

    /// Jumps to an index in the queue.
    pub fn jump(&mut self, index: usize) -> Option<&Track> {
        if index < self.tracks.len() {
            self.current = Some(index);
        }
        self.current()
    }

    /// Inserts a track right after the current one.
    pub fn play_next(&mut self, track: Track) {
        let at = self.current.map_or(self.tracks.len(), |c| c + 1);
        self.tracks.insert(at, track.clone());
        if let Some(orig) = &mut self.unshuffled {
            orig.push(track);
        }
        self.current.get_or_insert(0);
    }

    pub fn append(&mut self, tracks: impl IntoIterator<Item = Track>) {
        for t in tracks {
            if let Some(orig) = &mut self.unshuffled {
                orig.push(t.clone());
            }
            self.tracks.push(t);
        }
        if self.current.is_none() && !self.tracks.is_empty() {
            self.current = Some(0);
        }
    }

    /// Removes a track; the current track cannot be removed.
    pub fn remove(&mut self, index: usize) -> Option<Track> {
        let cur = self.current?;
        if index == cur || index >= self.tracks.len() {
            return None;
        }
        let removed = self.tracks.remove(index);
        if index < cur {
            self.current = Some(cur - 1);
        }
        if let Some(orig) = &mut self.unshuffled {
            if let Some(pos) = orig.iter().position(|t| t == &removed) {
                orig.remove(pos);
            }
        }
        Some(removed)
    }

    /// Swaps a track with its neighbour; returns the new index.
    pub fn move_by(&mut self, index: usize, up: bool) -> usize {
        let target = if up {
            index.checked_sub(1)
        } else {
            Some(index + 1).filter(|t| *t < self.tracks.len())
        };
        let Some(target) = target.filter(|_| index < self.tracks.len()) else {
            return index;
        };
        self.tracks.swap(index, target);
        self.current = self.current.map(|c| match c {
            c if c == index => target,
            c if c == target => index,
            c => c,
        });
        target
    }

    /// Shuffles upcoming tracks, or restores their original order.
    pub fn toggle_shuffle(&mut self, rng: &mut impl Rng) {
        let Some(cur) = self.current else { return };
        match self.unshuffled.take() {
            Some(original) => {
                let playing = self.tracks[cur].clone();
                self.current = original.iter().position(|t| t == &playing).or(Some(0));
                self.tracks = original;
            }
            None => {
                self.unshuffled = Some(self.tracks.clone());
                self.tracks[cur + 1..].shuffle(rng);
            }
        }
    }

    /// True when radio should append more tracks to keep playing.
    pub fn needs_radio(&self) -> bool {
        self.repeat == Repeat::Off
            && self
                .current
                .is_some_and(|c| self.tracks.len().saturating_sub(c + 1) < RADIO_THRESHOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn t(id: &str) -> Track {
        Track {
            video_id: id.into(),
            title: id.into(),
            artists: String::new(),
            album: None,
            duration: None,
            liked: None,
        }
    }

    fn ids(q: &Queue) -> Vec<String> {
        q.tracks().iter().map(|t| t.video_id.clone()).collect()
    }

    fn queue(n: usize) -> Queue {
        let mut q = Queue::default();
        q.set((0..n).map(|i| t(&i.to_string())).collect(), 0);
        q
    }

    #[test]
    fn test_set_clamps_start_and_handles_empty() {
        let mut q = Queue::default();
        q.set(vec![t("a"), t("b")], 9);
        assert_eq!(q.current_index(), Some(1));
        q.set(vec![], 0);
        assert!(q.current().is_none());
    }

    #[test]
    fn test_advance_respects_repeat_modes() {
        let mut q = queue(2);
        assert_eq!(q.advance(true).unwrap().video_id, "1");
        assert!(q.advance(true).is_none(), "repeat off stops at the end");
        q.set_repeat(Repeat::All);
        assert_eq!(q.advance(true).unwrap().video_id, "0");
        q.set_repeat(Repeat::One);
        assert_eq!(
            q.advance(true).unwrap().video_id,
            "0",
            "auto-advance repeats the song"
        );
        assert_eq!(
            q.advance(false).unwrap().video_id,
            "1",
            "manual next still moves"
        );
    }

    #[test]
    fn test_peek_next_does_not_move() {
        let q = queue(3);
        assert_eq!(q.peek_next().unwrap().video_id, "1");
        assert_eq!(q.current_index(), Some(0));
    }

    #[test]
    fn test_back_stops_at_first() {
        let mut q = queue(3);
        q.jump(2);
        assert_eq!(q.back().unwrap().video_id, "1");
        q.back();
        assert_eq!(q.back().unwrap().video_id, "0");
    }

    #[test]
    fn test_play_next_inserts_after_current() {
        let mut q = queue(3);
        q.jump(1);
        q.play_next(t("x"));
        assert_eq!(ids(&q), ["0", "1", "x", "2"]);
    }

    #[test]
    fn test_remove_adjusts_current_and_protects_playing() {
        let mut q = queue(4);
        q.jump(2);
        assert!(q.remove(2).is_none());
        assert_eq!(q.remove(0).unwrap().video_id, "0");
        assert_eq!(q.current().unwrap().video_id, "2");
        assert_eq!(q.current_index(), Some(1));
    }

    #[test]
    fn test_move_by_tracks_current() {
        let mut q = queue(3);
        assert_eq!(q.move_by(0, false), 1);
        assert_eq!(ids(&q), ["1", "0", "2"]);
        assert_eq!(q.current().unwrap().video_id, "0");
        assert_eq!(q.move_by(0, true), 0, "cannot move above the top");
        assert_eq!(q.move_by(2, false), 2, "cannot move below the bottom");
    }

    #[test]
    fn test_shuffle_keeps_played_part_and_restores_order() {
        let mut q = queue(10);
        q.jump(3);
        let mut rng = StdRng::seed_from_u64(7);
        q.toggle_shuffle(&mut rng);
        assert!(q.is_shuffled());
        assert_eq!(&ids(&q)[..4], ["0", "1", "2", "3"]);
        assert_ne!(
            ids(&q)[4..],
            (4..10).map(|i| i.to_string()).collect::<Vec<_>>()
        );
        q.advance(false);
        let playing = q.current().unwrap().video_id.clone();
        q.toggle_shuffle(&mut rng);
        assert!(!q.is_shuffled());
        assert_eq!(ids(&q), (0..10).map(|i| i.to_string()).collect::<Vec<_>>());
        assert_eq!(q.current().unwrap().video_id, playing);
    }

    #[test]
    fn test_needs_radio_near_the_end_only_without_repeat() {
        let mut q = queue(5);
        assert!(!q.needs_radio());
        q.jump(3);
        assert!(q.needs_radio());
        q.set_repeat(Repeat::All);
        assert!(!q.needs_radio());
        q.set_repeat(Repeat::Off);
        q.append([t("a"), t("b")]);
        assert!(!q.needs_radio());
    }

    #[test]
    fn test_repeat_cycle() {
        assert_eq!(Repeat::Off.cycle(), Repeat::All);
        assert_eq!(Repeat::All.cycle(), Repeat::One);
        assert_eq!(Repeat::One.cycle(), Repeat::Off);
    }
}
