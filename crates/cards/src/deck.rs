use super::card::Card;
use super::hand::Hand;
use super::hole::Hole;
use super::street::Street;

/// A mutable deck of cards supporting random draws.
///
/// Wraps a [`Hand`] representing the remaining cards, with methods for
/// randomly drawing cards and dealing hands. Used for Monte Carlo sampling
/// and game simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deck(Hand);

impl Default for Deck {
    fn default() -> Self {
        Self::new()
    }
}

impl Deck {
    /// Creates a fresh 52-card deck (or 36 for short deck).
    pub fn new() -> Self {
        Self(Hand::from(Hand::mask()))
    }
    /// Tests whether a card is still in the deck.
    pub fn contains(&self, card: &Card) -> bool {
        self.0.contains(card)
    }
    /// Draws and removes a uniformly random card from the deck.
    ///
    /// Unlike `Hand::next()` which is deterministic, this samples
    /// uniformly for Monte Carlo simulation.
    pub fn draw(&mut self) -> Card {
        let n = self.0.size();
        let i = rand::random_range(0..n) as u8;
        self.draw_index(i)
    }
    /// Draws and removes a card at the caller-supplied uniformly
    /// random index in `0..size`. Used by [`Deck::draw_with`] to
    /// thread a seeded RNG through the deal path so tests and
    /// property proofs can replay the exact same deal without
    /// depending on the global RNG.
    fn draw_index(&mut self, i: u8) -> Card {
        debug_assert!(self.0.size() > 0);
        let mut ones = 0u8;
        let mut deck = u64::from(self.0);
        let mut card = u64::from(self.0).trailing_zeros() as u8;
        while ones < i {
            card = deck.trailing_zeros() as u8;
            deck = deck & (deck - 1);
            ones = ones + 1;
        }
        let card = Card::from(card);
        self.0.remove(card);
        card
    }
    /// Deals the appropriate number of cards for the next street,
    /// sampling from the caller's seeded RNG (`StdRng::seed_from_u64`
    /// or any other `RngCore + CryptoRng`). Identical deal shape to
    /// [`Deck::deal`] but reproducible: the same `seed` always
    /// produces the same `Hand`. The hole cards in `Game::root()`
    /// still use the global RNG; this only determinizes the
    /// postflop street deals.
    pub fn deal_with<R: rand::Rng + ?Sized>(&mut self, rng: &mut R, street: Street) -> Hand {
        (0..street.next().n_revealed())
            .map(|_| {
                let n = self.0.size();
                let i = rng.random_range(0..n) as u8;
                self.draw_index(i)
            })
            .map(Hand::from)
            .fold(Hand::empty(), Hand::add)
    }
    /// Deals the appropriate number of cards for the next street.
    pub fn deal(&mut self, street: Street) -> Hand {
        (0..street.next().n_revealed())
            .map(|_| self.draw())
            .map(Hand::from)
            .fold(Hand::empty(), Hand::add)
    }
    /// Deals two cards as a player's hole cards.
    pub fn hole(&mut self) -> Hole {
        let a = self.draw();
        let b = self.draw();
        Hole::from((a, b))
    }
}

impl From<Deck> for Hand {
    fn from(deck: Deck) -> Self {
        deck.0
    }
}
impl From<Hand> for Deck {
    fn from(hand: Hand) -> Self {
        Self(hand)
    }
}

impl Iterator for Deck {
    type Item = Card;
    fn next(&mut self) -> Option<Self::Item> {
        Some(self.draw())
    }
}
