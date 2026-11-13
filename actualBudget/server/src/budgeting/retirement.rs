//! Compatibility boundary for existing databases and portable/Actual imports.
//! The active method's allocations become tracking plans; original archives remain intact.
use crate::error::Error;
use rusqlite::{Connection, params};

pub fn normalize(db: &Connection, only: Option<&str>) -> Result<(), Error> {
    let budgets = db
        .prepare("SELECT id FROM budgets WHERE budget_type='envelope' AND (?1 IS NULL OR id=?1)")?
        .query_map([only], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    for budget in budgets {
        // A previously inactive tracking plan must not leak into the active converted plan.
        // Epoch rotation forces clients to rebuild; remove change entries for replaced identities.
        db.execute("DELETE FROM budget_changes WHERE budget_id=?1 AND entity='budget_cells' AND entity_id LIKE 'tracking:%'", [&budget])?;
        db.execute(
            "DELETE FROM budget_cells WHERE budget_id=?1 AND method='tracking'",
            [&budget],
        )?;
        db.execute("DELETE FROM field_versions WHERE budget_id=?1 AND entity='budget_cells' AND entity_id LIKE 'tracking:%'", [&budget])?;
        db.execute("INSERT INTO budget_cells(budget_id,id,method,month,category_id,amount,carryover,goal,long_goal,tombstone) SELECT budget_id,'tracking:'||month||':'||category_id,'tracking',month,category_id,amount,carryover,goal,long_goal,tombstone FROM budget_cells WHERE budget_id=?1 AND method='envelope'", [&budget])?;
        db.execute("INSERT INTO field_versions(budget_id,entity,entity_id,field,clock) SELECT budget_id,entity,'tracking:'||substr(entity_id,10),field,clock FROM field_versions WHERE budget_id=?1 AND entity='budget_cells' AND entity_id LIKE 'envelope:%'", [&budget])?;
        db.execute(
            "UPDATE budget_settings SET method='tracking' WHERE budget_id=?1",
            [&budget],
        )?;
        db.execute(
            "UPDATE budgets SET budget_type='tracking' WHERE id=?1",
            [&budget],
        )?;
        // Old-method undo and offline snapshots cannot be applied to converted allocations.
        db.execute("UPDATE budget_sync SET epoch=lower(hex(randomblob(16))),sequence=sequence+1 WHERE budget_id=?1", [&budget])?;
        db.execute("DELETE FROM history_actions WHERE budget_id=?1", [&budget])?;
        db.execute("DELETE FROM history_lineage WHERE budget_id=?1", [&budget])?;
    }
    // Storage columns remain solely for decoding old archives. No active reader/writer uses them.
    db.execute("UPDATE budget_months SET buffered=0,notes='' WHERE (?1 IS NULL OR budget_id=?1) AND (buffered<>0 OR notes<>'')", [only])?;
    db.execute("UPDATE notes SET note='',tombstone=1 WHERE (?1 IS NULL OR budget_id=?1) AND (id GLOB 'budget-[0-9][0-9][0-9][0-9]-[0-9][0-9]' OR id=budget_id)", [only])?;
    db.execute("DELETE FROM history_actions WHERE (?1 IS NULL OR budget_id=?1) AND (changes LIKE '%\"budget_months\"%' OR changes LIKE '%\"budget_settings\"%')", params![only])?;
    Ok(())
}
