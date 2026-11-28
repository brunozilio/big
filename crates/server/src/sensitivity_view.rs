use app::labels;
use domain::label::{Levels, Mark, reads};
use domain::workflow::WorkItem;
use serde_json::{Value, json};

pub fn sensitivity(item: Option<&WorkItem>) -> Value {
    let Some(item) = item else { return Value::Null };
    let read = reads(item);
    if read.is_empty() {
        return Value::Null;
    }
    let levels = labels::levels();
    band(&levels, &Mark::of(&levels, &read))
}

pub fn band(levels: &Levels, mark: &Mark) -> Value {
    if mark.is_empty() {
        return Value::Null;
    }
    json!({
        "label": mark.label,
        "from": mark.from,
        "rank": mark.rank,
        "levels": levels.levels,
        "external": mark.external,
        "mode": labels::mode(levels).word(),
    })
}

#[cfg(test)]
mod tests {
    use domain::label::{Mode, Read, Trust};

    use super::*;

    #[test]
    fn the_band_shows_the_highest_label_where_it_came_from_and_the_external_sources() {
        let levels =
            Levels { mode: Mode::Shadow, levels: vec!["public".into(), "internal".into(), "confidential".into()] };
        let reads = [
            Read::new("Vault record", Trust::System, Some("confidential".into())),
            Read::new("Mail message", Trust::External, Some("internal".into())),
        ];
        let shown = band(&levels, &Mark::of(&levels, &reads));
        assert_eq!(
            shown,
            json!({
                "label": "confidential", "from": "Vault record", "rank": 2,
                "levels": ["public", "internal", "confidential"], "external": ["Mail message"], "mode": "shadow",
            })
        );
        assert_eq!(band(&levels, &Mark::default()), Value::Null);
        assert_eq!(sensitivity(None), Value::Null);
    }
}
