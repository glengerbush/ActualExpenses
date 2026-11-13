//! Stable identity-based shades; manual category overrides survive group recoloring.
use crate::error::Error;
use rusqlite::Connection;
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn hash(id: &str) -> usize {
    id.bytes()
        .fold(0usize, |n, c| n.wrapping_mul(31).wrapping_add(c as usize))
}
pub(super) fn group(id: &str) -> String {
    let palette = [
        "#397C8C", "#7764A0", "#A66B42", "#4D8563", "#AC5D74", "#6779A6", "#9A803C", "#587E78",
    ];
    palette[hash(id) % palette.len()].into()
}
fn shade(base: &str, id: &str) -> String {
    let shift = [-0.5, -0.3, -0.1, 0.15, 0.35, 0.55, 0.75][hash(id) % 7];
    let mut rgb = [0u8; 3];
    for (i, c) in rgb.iter_mut().enumerate() {
        let value = base
            .get(1 + i * 2..3 + i * 2)
            .and_then(|v| u8::from_str_radix(v, 16).ok())
            .unwrap_or(100) as f64;
        *c = if shift >= 0.0 {
            value + (255.0 - value) * shift
        } else {
            value * (1.0 + shift)
        }
        .round() as u8;
    }
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
pub(crate) fn decorate(
    db: &Connection,
    b: &str,
    rows: &mut [Value],
    key: &str,
) -> Result<(), Error> {
    // Ledger reads also run while enriching older schemas during migration.
    if db.query_row(
        "SELECT count(*) FROM pragma_table_info('categories') WHERE name='color'",
        [],
        |r| r.get::<_, i64>(0),
    )? == 0
    {
        return Ok(());
    }
    for batch in rows.chunks_mut(500) {
        let mut args = vec![b.to_owned()];
        args.extend(
            batch
                .iter()
                .filter_map(|r| r[key].as_str().map(str::to_owned)),
        );
        if args.len() == 1 {
            continue;
        }
        let slots = (2..=args.len())
            .map(|n| format!("?{n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut colors = BTreeMap::new();
        let mut q=db.prepare(&format!("SELECT c.id,c.color,c.group_id,g.color,g.sort_order FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1 AND c.id IN ({slots})"))?;
        for row in q.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<f64>>(4)?,
            ))
        })? {
            let (id, override_color, group_id, group_color, group_order) = row?;
            let base =
                group_color.unwrap_or_else(|| group(group_id.as_deref().unwrap_or("ungrouped")));
            let color = override_color.clone().unwrap_or_else(|| shade(&base, &id));
            colors.insert(id, (color, base, override_color, group_order));
        }
        for row in batch {
            if let Some((color, group, manual, order)) =
                row[key].as_str().and_then(|id| colors.get(id))
            {
                row["category_color"] = json!(color);
                row["category_group_color"] = json!(group);
                row["color_override"] = json!(manual);
                row["group_sort_order"] = json!(order);
            }
        }
    }
    Ok(())
}
