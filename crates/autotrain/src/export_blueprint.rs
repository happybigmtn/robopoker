//! Postgres -> Arena Starter Kit blueprint JSON bridge.
//!
//! The Arena side already has a conservative `--blueprint-path`
//! seam that reads a JSON file and falls back to the L1 floor when a
//! key is missing. This module writes that file from the trained
//! robopoker blueprint tables. The first bridge is intentionally
//! preflop-only: robopoker persists abstract buckets, while the
//! starter kit keys decisions by concrete L1 hand classes.

use rbp_cards::{Board, Hole, Isomorphism, Observation, Street};
use rbp_core::Chips;
use rbp_database::{BLUEPRINT, BLUEPRINT2, BLUEPRINT3, ISOMORPHISM};
use rbp_gameplay::{Abstraction, Edge, Path};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path as FsPath, PathBuf};
use tokio_postgres::Client;

pub const EXPORT_BLUEPRINT_HEADLINE_PREFIX: &str = "blueprint export complete:";
pub const EXPORT_BLUEPRINT_SCHEMA: &str = "l1_v1";
const DEFAULT_EXPORT_BIG_BLIND_CHIPS: Chips = 20;

/// Trained-config variant to export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBlueprintVariant {
    V1,
    V2,
    V3,
}

impl ExportBlueprintVariant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::V2 => "v2",
            Self::V3 => "v3",
        }
    }

    pub fn table_name(self) -> &'static str {
        match self {
            Self::V1 => BLUEPRINT,
            Self::V2 => BLUEPRINT2,
            Self::V3 => BLUEPRINT3,
        }
    }

    pub fn from_arg(raw: &str) -> Option<Self> {
        match raw {
            "v1" => Some(Self::V1),
            "v2" => Some(Self::V2),
            "v3" => Some(Self::V3),
            _ => None,
        }
    }
}

impl Default for ExportBlueprintVariant {
    fn default() -> Self {
        Self::V3
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintArtifact {
    pub header: BlueprintHeader,
    pub entries: Vec<BlueprintEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintHeader {
    pub source: String,
    pub coarse: bool,
    pub schema: String,
    pub entries: usize,
    pub bridge_shape: String,
    pub variant: String,
    pub table: String,
    pub street_scope: String,
    pub action_policy: String,
    pub amount_base_big_blind_chips: Chips,
    pub generated_at_utc: String,
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BlueprintKey {
    pub street: String,
    pub position: String,
    pub hand_class: String,
    pub facing_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintEntry {
    pub key: BlueprintKey,
    pub action: String,
    pub amount_chips: Option<Chips>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportBlueprintReport {
    pub path: PathBuf,
    pub variant: ExportBlueprintVariant,
    pub source_rows: usize,
    pub entries: usize,
    pub skipped_rows: usize,
}

impl ExportBlueprintReport {
    pub fn headline(&self) -> String {
        format!(
            "{EXPORT_BLUEPRINT_HEADLINE_PREFIX} path={} blueprint={} entries={} source_rows={} skipped_rows={} schema={EXPORT_BLUEPRINT_SCHEMA}\n",
            self.path.display(),
            self.variant.as_str(),
            self.entries,
            self.source_rows,
            self.skipped_rows,
        )
    }
}

#[derive(Debug, Clone)]
struct BlueprintRow {
    past: Path,
    present: Abstraction,
    position: usize,
    edge: Edge,
    weight: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct InfoKey {
    past: Path,
    present: Abstraction,
    position: usize,
}

pub async fn export_blueprint(
    client: &Client,
    output_path: &FsPath,
    variant: ExportBlueprintVariant,
) -> Result<ExportBlueprintReport, String> {
    if output_path.as_os_str().is_empty() {
        return Err("missing export path".to_string());
    }
    let rows = load_blueprint_rows(client, variant).await?;
    let source_rows = rows.len();
    let bucket_classes = load_preflop_bucket_classes(client).await?;
    if bucket_classes.is_empty() {
        return Err(
            "no preflop hand-class bucket mapping found; run trainer --cluster first".into(),
        );
    }
    let amount_bb = export_big_blind_chips();
    let artifact = build_artifact(&rows, &bucket_classes, variant, amount_bb);
    write_artifact_atomic(output_path, &artifact)?;
    Ok(ExportBlueprintReport {
        path: output_path.to_path_buf(),
        variant,
        source_rows,
        entries: artifact.entries.len(),
        skipped_rows: source_rows.saturating_sub(count_exportable_rows(&rows)),
    })
}

async fn load_blueprint_rows(
    client: &Client,
    variant: ExportBlueprintVariant,
) -> Result<Vec<BlueprintRow>, String> {
    let sql = format!(
        "SELECT past, present, choices, position, edge, weight \
         FROM {} \
         ORDER BY past, present, choices, position, edge",
        variant.table_name()
    );
    let rows = client
        .query(&sql, &[])
        .await
        .map_err(|e| format!("query {}: {e}", variant.table_name()))?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let _choices: i64 = row.get(2);
            BlueprintRow {
                past: Path::from(row.get::<_, i64>(0)),
                present: Abstraction::from(row.get::<_, i16>(1)),
                position: row.get::<_, i16>(3) as usize,
                edge: Edge::from(row.get::<_, i64>(4) as u64),
                weight: row.get::<_, f32>(5),
            }
        })
        .collect())
}

async fn load_preflop_bucket_classes(
    client: &Client,
) -> Result<BTreeMap<Abstraction, Vec<String>>, String> {
    let sql = format!("SELECT obs, abs FROM {ISOMORPHISM}");
    let rows = client
        .query(&sql, &[])
        .await
        .map_err(|e| format!("query {ISOMORPHISM}: {e}"))?;
    let lookup = rows
        .into_iter()
        .map(|row| {
            (
                Isomorphism::from(row.get::<_, i64>(0)),
                Abstraction::from(row.get::<_, i16>(1)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    Ok(preflop_hand_classes_by_bucket(&lookup))
}

fn build_artifact(
    rows: &[BlueprintRow],
    bucket_classes: &BTreeMap<Abstraction, Vec<String>>,
    variant: ExportBlueprintVariant,
    amount_bb: Chips,
) -> BlueprintArtifact {
    let mut grouped: BTreeMap<InfoKey, Vec<&BlueprintRow>> = BTreeMap::new();
    for row in rows.iter().filter(|row| is_preflop_row(row)) {
        grouped
            .entry(InfoKey {
                past: row.past,
                present: row.present,
                position: row.position,
            })
            .or_default()
            .push(row);
    }

    let mut entries_by_key: BTreeMap<BlueprintKey, BlueprintEntry> = BTreeMap::new();
    for (info, candidates) in grouped {
        let Some(classes) = bucket_classes.get(&info.present) else {
            continue;
        };
        let Some(best) = best_weighted_edge(&candidates) else {
            continue;
        };
        let facing_action = facing_action_from_past(info.past);
        let Some((action, amount_chips)) =
            edge_to_entry_action(best.edge, facing_action, amount_bb)
        else {
            continue;
        };
        for position in position_labels(info.position) {
            for hand_class in classes {
                let key = BlueprintKey {
                    street: Street::Pref.label().to_string(),
                    position: position.to_string(),
                    hand_class: hand_class.clone(),
                    facing_action: facing_action.to_string(),
                };
                entries_by_key.insert(
                    key.clone(),
                    BlueprintEntry {
                        key,
                        action: action.to_string(),
                        amount_chips,
                    },
                );
            }
        }
    }

    let entries = entries_by_key.into_values().collect::<Vec<_>>();
    BlueprintArtifact {
        header: BlueprintHeader {
            source: "robopoker-postgres".to_string(),
            coarse: true,
            schema: EXPORT_BLUEPRINT_SCHEMA.to_string(),
            entries: entries.len(),
            bridge_shape: "postgres_to_json".to_string(),
            variant: variant.as_str().to_string(),
            table: variant.table_name().to_string(),
            street_scope: "preflop".to_string(),
            action_policy: "argmax_cumulative_weight".to_string(),
            amount_base_big_blind_chips: amount_bb,
            generated_at_utc: std::env::var("RBP_EXPORT_BLUEPRINT_UTC")
                .unwrap_or_else(|_| "unknown".to_string()),
            notes: "Preflop-only projection into the Arena Starter Kit l1_v1 seam; missing keys fall back to the L1 floor.".to_string(),
        },
        entries,
    }
}

fn count_exportable_rows(rows: &[BlueprintRow]) -> usize {
    rows.iter().filter(|row| is_preflop_row(row)).count()
}

fn is_preflop_row(row: &BlueprintRow) -> bool {
    row.present.street() == Street::Pref && row.past.street() == Street::Pref
}

fn best_weighted_edge<'a>(rows: &'a [&BlueprintRow]) -> Option<&'a BlueprintRow> {
    rows.iter()
        .copied()
        .filter(|row| row.weight.is_finite() && row.weight > 0.0)
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
}

fn facing_action_from_past(past: Path) -> &'static str {
    let choices = Vec::<Edge>::from(past)
        .into_iter()
        .filter(Edge::is_choice)
        .collect::<Vec<_>>();
    let aggro = choices.iter().filter(|edge| edge.is_aggro()).count();
    if aggro == 0 {
        if choices.iter().any(|edge| matches!(edge, Edge::Call)) {
            "limp"
        } else if choices.iter().any(|edge| matches!(edge, Edge::Check)) {
            "check"
        } else {
            "open"
        }
    } else if aggro == 1 {
        "raise3bet"
    } else {
        "raise4bet"
    }
}

fn edge_to_entry_action(
    edge: Edge,
    facing_action: &str,
    amount_bb: Chips,
) -> Option<(&'static str, Option<Chips>)> {
    match edge {
        Edge::Fold => Some(("fold", None)),
        Edge::Check => Some(("check", None)),
        Edge::Call => Some(("call", None)),
        Edge::Open(n) => Some(("raise", Some(n * amount_bb))),
        Edge::Raise(_) => {
            let multiplier = match facing_action {
                "raise3bet" => 6,
                "raise4bet" => 12,
                _ => 4,
            };
            Some(("raise", Some(multiplier * amount_bb)))
        }
        Edge::Shove | Edge::Draw => None,
    }
}

fn position_labels(position: usize) -> &'static [&'static str] {
    match position {
        0 => &["SB"],
        1 => &["BTN", "BB"],
        2 => &["CO"],
        3 => &["MP"],
        _ => &["UTG"],
    }
}

fn preflop_hand_classes_by_bucket(
    lookup: &BTreeMap<Isomorphism, Abstraction>,
) -> BTreeMap<Abstraction, Vec<String>> {
    let mut out: BTreeMap<Abstraction, Vec<String>> = BTreeMap::new();
    for class in preflop_hand_classes() {
        let obs = observation_for_hand_class(&class);
        let iso = Isomorphism::from(obs);
        if let Some(abs) = lookup.get(&iso).copied() {
            out.entry(abs).or_default().push(class);
        }
    }
    out
}

fn preflop_hand_classes() -> Vec<String> {
    let ranks = [
        "A", "K", "Q", "J", "T", "9", "8", "7", "6", "5", "4", "3", "2",
    ];
    let mut classes = Vec::with_capacity(169);
    for (i, hi) in ranks.iter().enumerate() {
        for lo in ranks.iter().skip(i) {
            if hi == lo {
                classes.push(format!("{hi}{lo}"));
            } else {
                classes.push(format!("{hi}{lo}s"));
                classes.push(format!("{hi}{lo}o"));
            }
        }
    }
    classes
}

fn observation_for_hand_class(class: &str) -> Observation {
    let mut chars = class.chars();
    let high = chars.next().expect("hand class rank 1");
    let low = chars.next().expect("hand class rank 2");
    let suffix = chars.next();
    let cards = if high == low {
        format!("{high}s {low}h")
    } else if suffix == Some('s') {
        format!("{high}s {low}s")
    } else {
        format!("{high}s {low}h")
    };
    let hole = Hole::try_from(cards.as_str()).expect("generated hand class must parse");
    Observation::from((hole, Board::empty()))
}

fn export_big_blind_chips() -> Chips {
    std::env::var("RBP_EXPORT_BLUEPRINT_BIG_BLIND_CHIPS")
        .ok()
        .and_then(|raw| raw.trim().parse::<Chips>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_EXPORT_BIG_BLIND_CHIPS)
}

fn write_artifact_atomic(path: &FsPath, artifact: &BlueprintArtifact) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(artifact)
        .map_err(|e| format!("serialize blueprint artifact: {e}"))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("blueprint.json");
    let tmp = path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()));
    std::fs::write(&tmp, format!("{json}\n"))
        .map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("rename {} -> {}: {e}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_gameplay::Odds;

    fn row(
        past: Path,
        present: Abstraction,
        position: usize,
        edge: Edge,
        weight: f32,
    ) -> BlueprintRow {
        BlueprintRow {
            past,
            present,
            position,
            edge,
            weight,
        }
    }

    #[test]
    fn export_variant_defaults_to_latest_v3() {
        assert_eq!(
            ExportBlueprintVariant::default(),
            ExportBlueprintVariant::V3
        );
        assert_eq!(ExportBlueprintVariant::V3.table_name(), BLUEPRINT3);
        assert_eq!(
            ExportBlueprintVariant::from_arg("v1"),
            Some(ExportBlueprintVariant::V1)
        );
        assert_eq!(
            ExportBlueprintVariant::from_arg("v2"),
            Some(ExportBlueprintVariant::V2)
        );
        assert_eq!(
            ExportBlueprintVariant::from_arg("v3"),
            Some(ExportBlueprintVariant::V3)
        );
        assert_eq!(ExportBlueprintVariant::from_arg("latest"), None);
    }

    #[test]
    fn preflop_hand_class_projection_has_169_classes() {
        let classes = preflop_hand_classes();
        assert_eq!(classes.len(), 169);
        assert!(classes.contains(&"AA".to_string()));
        assert!(classes.contains(&"AKs".to_string()));
        assert!(classes.contains(&"AKo".to_string()));
        assert!(classes.contains(&"72o".to_string()));
    }

    #[test]
    fn facing_action_maps_preflop_history_to_starter_kit_axis() {
        assert_eq!(facing_action_from_past(Path::default()), "open");
        assert_eq!(facing_action_from_past(vec![Edge::Call].into()), "limp");
        assert_eq!(
            facing_action_from_past(vec![Edge::Open(2)].into()),
            "raise3bet"
        );
        assert_eq!(
            facing_action_from_past(vec![Edge::Open(2), Edge::Raise(Odds::new(1, 1))].into()),
            "raise4bet"
        );
    }

    #[test]
    fn edge_to_entry_action_uses_arena_chip_amounts_for_sized_preflop_actions() {
        assert_eq!(
            edge_to_entry_action(Edge::Fold, "open", 20),
            Some(("fold", None))
        );
        assert_eq!(
            edge_to_entry_action(Edge::Call, "raise3bet", 20),
            Some(("call", None))
        );
        assert_eq!(
            edge_to_entry_action(Edge::Open(3), "open", 20),
            Some(("raise", Some(60)))
        );
        assert_eq!(
            edge_to_entry_action(Edge::Raise(Odds::new(1, 1)), "raise3bet", 20),
            Some(("raise", Some(120)))
        );
        assert_eq!(edge_to_entry_action(Edge::Shove, "raise4bet", 20), None);
    }

    #[test]
    fn build_artifact_writes_starter_kit_l1_v1_shape() {
        let bucket = Abstraction::from((Street::Pref, 7));
        let mut bucket_classes = BTreeMap::new();
        bucket_classes.insert(bucket, vec!["AJo".to_string()]);
        let rows = vec![
            row(Path::default(), bucket, 1, Edge::Fold, 1.0),
            row(Path::default(), bucket, 1, Edge::Call, 2.0),
            row(Path::default(), bucket, 1, Edge::Open(3), 5.0),
        ];

        let artifact = build_artifact(&rows, &bucket_classes, ExportBlueprintVariant::V3, 20);
        assert_eq!(artifact.header.schema, "l1_v1");
        assert_eq!(artifact.header.variant, "v3");
        assert_eq!(artifact.header.entries, 2);
        assert_eq!(artifact.entries.len(), 2);
        assert!(artifact.entries.iter().any(|entry| {
            entry.key
                == BlueprintKey {
                    street: "Preflop".to_string(),
                    position: "BTN".to_string(),
                    hand_class: "AJo".to_string(),
                    facing_action: "open".to_string(),
                }
                && entry.action == "raise"
                && entry.amount_chips == Some(60)
        }));
        assert!(
            artifact
                .entries
                .iter()
                .any(|entry| entry.key.position == "BB")
        );

        let json = serde_json::to_string(&artifact).expect("serialise artifact");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse artifact");
        assert_eq!(parsed["header"]["schema"], "l1_v1");
        assert_eq!(parsed["entries"][0]["key"]["street"], "Preflop");
        assert!(parsed["entries"][0]["action"].is_string());
    }
}
