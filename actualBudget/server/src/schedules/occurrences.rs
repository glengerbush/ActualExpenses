//! Read-only calendar pages do not create completion evidence or receipts.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    from: Option<String>,
    through: Option<String>,
    #[serde(default = "limit")]
    limit: usize,
    snapshot: Option<String>,
}
fn limit() -> usize {
    100
}
pub fn dates(
    row: &Value,
    from: &str,
    through: &str,
    limit: usize,
    work: &mut usize,
) -> Result<(Vec<String>, bool), Error> {
    crate::models::validate_date(from)?;
    crate::models::validate_date(through)?;
    if from > through {
        return Ok((vec![], false));
    }
    let value = &calendar::date_condition(row["conditions"].as_array().unwrap())?["value"];
    // Include raw weekend dates just before an adjusted starting Monday. Filter
    // afterward, preserving the source's merged adjusted-day occurrence identity.
    let raw_from = if from > "0001-01-03" {
        calendar::shift(from, -2)?
    } else {
        "0001-01-01".into()
    };
    let mut dates = calendar::dates(value, &raw_from, (limit + 1) * 3 + 6, work)?
        .into_iter()
        .filter(|d| d.as_str() >= from && d.as_str() <= through)
        .collect::<BTreeSet<_>>();
    if let Some(date) = row["next_date"].as_str()
        && date >= from
        && date <= through
    {
        dates.insert(date.to_owned());
    }
    let more = dates.len() > limit;
    Ok((dates.into_iter().take(limit).collect(), more))
}
pub async fn read(
    State(store): State<Store>,
    Path((b, id)): Path<(String, String)>,
    query: Result<Query<Page>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    crate::models::validate_id(&id)?;
    let Query(query) = query.map_err(|_| Error::invalid("Invalid occurrence page"))?;
    if query.limit == 0 || query.limit > 1000 {
        return Err(Error::invalid("Occurrence pages contain1–1000 dates"));
    }
    Ok(Json(store.run(move|db|{
  sync::check_snapshot(db,&b,query.snapshot.as_deref())?;
  let row=data::get(db,&b,&id)?;let today=calendar::day()?;
  let from=query.from.as_deref().or_else(||row["next_date"].as_str()).unwrap_or(&today);
  let horizon:String=db.query_row("SELECT value FROM schedule_settings WHERE budget_id=?1 AND id='horizon'",[&b],|r|r.get(0))?;
  let through=query.through.unwrap_or(calendar::shift(&today,view::horizon_days(row["custom_upcoming_length"].as_str().unwrap_or(&horizon),&today)?)?);
  let (dates,more)=dates(&row,from,&through,query.limit,&mut 2_000_000)?;
  let mut records=BTreeMap::new();
  for batch in dates.chunks(499){
   if batch.is_empty(){continue;}
   let placeholders=(3..=batch.len()+2).map(|n|format!("?{n}")).collect::<Vec<_>>().join(",");
   let mut args=vec![b.clone(),id.clone()];args.extend(batch.iter().cloned());
   records.extend(db.prepare(&format!("SELECT occurrence_date,status,transaction_id FROM schedule_occurrences WHERE budget_id=?1 AND schedule_id=?2 AND tombstone=0 AND occurrence_date IN({placeholders})"))?.query_map(rusqlite::params_from_iter(args.iter()),|r|Ok((r.get::<_,String>(0)?,(r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))))?.collect::<Result<BTreeMap<_,_>,_>>()?);
  }
  let current=row["next_date"].as_str();let linked=if let Some(date)=current{view::matched_transaction(db,&b,&row,date)?}else{None};
  let occurrences=dates.iter().map(|date|{
   let (status,transaction)=if let Some(record)=records.get(date){(record.0.as_str(),record.1.clone())}
    else if Some(date.as_str())==current&&linked.is_some(){("matched",linked.clone())}
    else if row["completed"]==true{("completed",None)}else if current.is_some_and(|d|date.as_str()<d){("past",None)}else{("pending",None)};
   json!({"id":data::identity(&["actualBudget.schedule-occurrence.v1",&b,&id,date]),"date":date,"status":status,"transaction_id":transaction})
  }).collect::<Vec<_>>();
  let next=if more{dates.last().map(|date|calendar::shift(date,1)).transpose()?}else{None};
  Ok(json!({"data":{"schedule_id":id,"occurrences":occurrences,"next_from":next,"snapshot":sync::cursor(db,&b)?,"epoch":crate::lifecycle::epoch(db,&b)?}}))
 }).await?))
}
