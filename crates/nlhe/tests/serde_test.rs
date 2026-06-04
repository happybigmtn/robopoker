//! Test serde serialization/deserialization for NLHE types
#![cfg(feature = "serde")]

use rbp_cards::*;
use rbp_core::Arbitrary;
use rbp_gameplay::*;
use rbp_nlhe::*;

#[test]
fn test_nlhe_info_roundtrip() {
    let info = NlheInfo::random();
    let serialized = serde_json::to_string(&info).expect("serialize NlheInfo");
    let deserialized: NlheInfo = serde_json::from_str(&serialized).expect("deserialize NlheInfo");

    assert_eq!(info.subgame(), deserialized.subgame());
    assert_eq!(info.bucket(), deserialized.bucket());
    assert_eq!(info.choices(), deserialized.choices());
}

#[test]
fn test_nlhe_edge_roundtrip() {
    let edges = vec![
        NlheEdge::from(Edge::Fold),
        NlheEdge::from(Edge::Call),
        NlheEdge::from(Edge::Check),
        NlheEdge::from(Edge::Shove),
        NlheEdge::from(Odds::new(1, 2)),
        NlheEdge::from(Odds::new(2, 1)),
    ];

    for edge in edges {
        let serialized = serde_json::to_string(&edge).expect("serialize NlheEdge");
        let deserialized: NlheEdge =
            serde_json::from_str(&serialized).expect("deserialize NlheEdge");
        assert_eq!(edge, deserialized);
    }
}

#[test]
fn test_nlhe_profile_iterations_roundtrip() {
    // Test that basic profile fields serialize correctly
    // Note: The encounters BTreeMap uses NlheInfo as keys which can't be
    // JSON-serialized directly. For wire format, use bincode or a Vec<(K, V)> representation.
    let mut profile = NlheProfile::default();
    profile.iterations = 100;

    let serialized = serde_json::to_string(&profile.iterations).expect("serialize iterations");
    let deserialized: usize = serde_json::from_str(&serialized).expect("deserialize iterations");

    assert_eq!(profile.iterations, deserialized);
}

#[test]
fn test_nlhe_profile_with_bincode() {
    // Use bincode for full profile serialization since it supports non-string map keys
    let mut profile = NlheProfile::default();
    profile.iterations = 100;

    // Add some encounters
    let info = NlheInfo::random();
    let edge = NlheEdge::from(Edge::Call);
    let encounters = profile.encounters.entry(info).or_default();
    encounters.insert(edge, rbp_mccfr::Encounter::new(0.5, 0.1, 0.2, 10));

    // Bincode supports non-string map keys
    let serialized = bincode::serialize(&profile).expect("serialize NlheProfile with bincode");
    let deserialized: NlheProfile =
        bincode::deserialize(&serialized).expect("deserialize NlheProfile with bincode");

    assert_eq!(profile.iterations, deserialized.iterations);
    assert_eq!(profile.encounters.len(), deserialized.encounters.len());
}

#[test]
fn test_path_roundtrip() {
    let edges: Vec<Edge> = vec![Edge::Fold, Edge::Call, Edge::Raise(Odds::new(1, 2))];
    let path: Path = edges.into_iter().collect();

    let serialized = serde_json::to_string(&path).expect("serialize Path");
    let deserialized: Path = serde_json::from_str(&serialized).expect("deserialize Path");

    assert_eq!(path, deserialized);
}

#[test]
fn test_edge_roundtrip() {
    let edges = vec![
        Edge::Fold,
        Edge::Call,
        Edge::Check,
        Edge::Shove,
        Edge::Raise(Odds::new(1, 2)),
        Edge::Open(2),
    ];

    for edge in edges {
        let serialized = serde_json::to_string(&edge).expect("serialize Edge");
        let deserialized: Edge = serde_json::from_str(&serialized).expect("deserialize Edge");
        assert_eq!(edge, deserialized);
    }
}

#[test]
fn test_abstraction_roundtrip() {
    let abstraction = Abstraction::from((Street::Flop, 42));

    let serialized = serde_json::to_string(&abstraction).expect("serialize Abstraction");
    let deserialized: Abstraction =
        serde_json::from_str(&serialized).expect("deserialize Abstraction");

    assert_eq!(abstraction, deserialized);
}

#[test]
fn test_card_roundtrip() {
    let card = Card::from((Rank::Ace, Suit::S));

    let serialized = serde_json::to_string(&card).expect("serialize Card");
    let deserialized: Card = serde_json::from_str(&serialized).expect("deserialize Card");

    assert_eq!(card, deserialized);
}

#[test]
fn test_hand_roundtrip() {
    // Convert cards to hands and combine them
    let hand1: Hand = Card::from((Rank::Ace, Suit::S)).into();
    let hand2: Hand = Card::from((Rank::King, Suit::H)).into();
    let hand = Hand::or(hand1, hand2);

    let serialized = serde_json::to_string(&hand).expect("serialize Hand");
    let deserialized: Hand = serde_json::from_str(&serialized).expect("deserialize Hand");

    assert_eq!(hand, deserialized);
}

#[test]
fn test_street_roundtrip() {
    for street in [Street::Pref, Street::Flop, Street::Turn, Street::Rive] {
        let serialized = serde_json::to_string(&street).expect("serialize Street");
        let deserialized: Street = serde_json::from_str(&serialized).expect("deserialize Street");
        assert_eq!(street, deserialized);
    }
}
