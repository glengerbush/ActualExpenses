//! Read-only matching uses the shared pinned condition evaluator, not posting.
use super::*;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    snapshot: String,
    conditions: Vec<Value>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
}
fn limit() -> usize {
    100
}
pub(super) async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    if body.conditions.is_empty()
        || body.conditions.len() > 32
        || body.limit == 0
        || body.limit > 1000
        || body.offset > 100000
    {
        return Err(Error::invalid(
            "Match1–32 conditions and page1–1000 entries",
        ));
    }
    crate::rules::validate_definition(&crate::rules::Rule {
        id: data::identity(&["schedule-match"]),
        stage: None,
        conditions_op: "and".into(),
        conditions: body.conditions.clone(),
        actions: vec![],
    })?;
    Ok(Json(store.run(move|db|{
        sync::check_snapshot(db,&b,Some(&body.snapshot))?;
        let account=body.conditions.iter().find(|c|c["field"]=="account"&&c["op"]=="is").and_then(|c|c["value"].as_str());
        let ids=db.prepare("SELECT id FROM transactions WHERE budget_id=?1 AND tombstone=0 AND (?2 IS NULL OR account_id=?2) ORDER BY date DESC,id LIMIT 100001")?.query_map(params![b,account],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
        if ids.len()>100000{return Err(Error::invalid("Matching exceeds100000 candidates. Narrow the account condition."));}
        let accounts=db.prepare("SELECT id,off_budget FROM accounts WHERE budget_id=?1")?.query_map([&b],|r|Ok((r.get::<_,String>(0)?,r.get::<_,bool>(1)?)))?.collect::<Result<std::collections::BTreeMap<_,_>,_>>()?;
        let mut work=crate::rules::Work::default();let mut total=0usize;let mut page=vec![];
        for batch in ids.chunks(500){
            for row in crate::ledger::get_many(db,&b,batch)?{
                let off=row["account_id"].as_str().and_then(|id|accounts.get(id)).copied();
                if crate::rules::match_schedule_conditions(db,&b,&body.conditions,&row,off,&mut work)?{
                    if total>=body.offset&&page.len()<body.limit {page.push(row);}
                    total+=1;
                }
            }
        }
        Ok(json!({"data":{"transactions":page,"total_count":total,"next_offset":(body.offset+body.limit<total).then_some(body.offset+body.limit),"snapshot":sync::cursor(db,&b)?,"epoch":crate::lifecycle::epoch(db,&b)?}}))
    }).await?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LinkedPage {
    snapshot: String,
    #[serde(default)]
    offset: usize,
    #[serde(default = "limit")]
    limit: usize,
}
pub(super) async fn linked(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    query: Result<Query<LinkedPage>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let Query(page) = query.map_err(|_| Error::invalid("Invalid linked transaction page"))?;
    if page.limit == 0 || page.limit > 1000 || page.offset > 100000 {
        return Err(Error::invalid("Read1–1000 linked transactions per page"));
    }
    Ok(Json(store.run(move|db|{
        sync::check_snapshot(db,&b,Some(&page.snapshot))?;data::get(db,&b,&id)?;
        let total:i64=db.query_row("SELECT COUNT(*) FROM schedule_links l JOIN transactions t ON t.budget_id=l.budget_id AND t.id=l.id WHERE l.budget_id=?1 AND l.schedule_id=?2 AND l.tombstone=0 AND t.tombstone=0",params![b,id],|r|r.get(0))?;
        let ids=db.prepare("SELECT t.id FROM schedule_links l JOIN transactions t ON t.budget_id=l.budget_id AND t.id=l.id WHERE l.budget_id=?1 AND l.schedule_id=?2 AND l.tombstone=0 AND t.tombstone=0 ORDER BY t.date DESC,t.id LIMIT ?3 OFFSET ?4")?.query_map(params![b,id,page.limit as i64,page.offset as i64],|r|r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
        Ok(json!({"data":{"transactions":crate::ledger::get_many(db,&b,&ids)?,"total_count":total,"next_offset":(((page.offset+page.limit) as i64)<total).then_some(page.offset+page.limit),"snapshot":sync::cursor(db,&b)?,"epoch":crate::lifecycle::epoch(db,&b)?}}))
    }).await?))
}
