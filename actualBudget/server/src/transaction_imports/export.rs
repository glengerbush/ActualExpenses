//! CSV is a bounded read, not another mutation or a whole-budget backup.
use super::*;
use std::io::Write;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    snapshot: String,
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    query: crate::register::Query,
    #[serde(default)]
    ids: Option<Vec<String>>,
    #[serde(default = "detailed")]
    layout: String,
}
fn detailed() -> String {
    "detailed".into()
}
struct Output(Vec<u8>);
impl Write for Output {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > 64 * 1024 * 1024 {
            return Err(std::io::Error::other(
                "CSV exceeds64 MiB; narrow the date/account filter",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn string(value: &str) -> String {
    if value.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{value}")
    } else {
        value.into()
    }
}
fn amount(value: i64) -> String {
    let value = value as i128;
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    let rest = value % 100;
    if rest == 0 {
        format!("{sign}{}", value / 100)
    } else if rest % 10 == 0 {
        format!("{sign}{}.{}", value / 100, rest / 10)
    } else {
        format!("{sign}{}.{rest:02}", value / 100)
    }
}
pub(super) async fn download(
    State(store): State<Store>,
    Path(b): Path<String>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<impl axum::response::IntoResponse, Error> {
    crate::models::validate_id(&b)?;
    let Json(body) = input.map_err(crate::api::json_error)?;
    if let Some(account) = body.account_id.as_deref() {
        crate::models::validate_id(account)?;
    }
    if !matches!(body.layout.as_str(), "standard" | "detailed") {
        return Err(Error::invalid(
            "Choose standard totals or detailed allocations CSV",
        ));
    }
    let bytes=store.run(move|db|{let tx=db.transaction()?;sync::check_snapshot(&tx,&b,Some(&body.snapshot))?;let standard=body.layout=="standard";let ids=crate::register::export_ids(&tx,&b,body.account_id.as_deref(),&body.query,body.ids.as_deref(),standard)?;let rows=crate::ledger::export_rows(&tx,&b,&ids)?;if rows.is_empty(){return Ok(vec![]);}
  let mut category_groups=BTreeMap::new();let mut q=tx.prepare("SELECT c.id,g.name FROM categories c LEFT JOIN category_groups g ON g.budget_id=c.budget_id AND g.id=c.group_id WHERE c.budget_id=?1")?;for row in q.query_map([&b],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?)))?{let(id,name)=row?;category_groups.insert(id,name.unwrap_or_default());}
  let mut totals=BTreeMap::<String,usize>::new();for row in &rows{if let Some(parent)=row["parent_id"].as_str(){*totals.entry(parent.into()).or_default()+=1;}}
  let mut positions=BTreeMap::<String,usize>::new();let mut writer=csv::WriterBuilder::new().from_writer(Output(vec![]));writer.write_record(if standard{vec!["Account","Date","Payee","Notes","Category","Amount","Cleared","Reconciled"]}else{vec!["Account","Date","Payee","Notes","Category_Group","Category","Amount","Split_Amount","Cleared"]}).map_err(|e|Error::invalid(e.to_string()))?;
  for row in rows{let text=|field:&str|row[field].as_str().unwrap_or("");let group=row["category_id"].as_str().and_then(|id|category_groups.get(id)).map(String::as_str).unwrap_or("");let category=text("category_name");let money=row["amount"].as_i64().unwrap();let mut fields=vec![string(text("account_name")),string(text("date")),string(text("payee_name"))];
   if standard{fields.push(string(text("notes")));fields.push(if category.is_empty(){String::new()}else{string(&format!("{group}: {category}"))});fields.extend([amount(money),if row["cleared"]==true{"1"}else{""}.into(),if row["reconciled"]==true{"1"}else{""}.into()]);}
   else{let notes=if row["is_parent"]==true{format!("(SPLIT INTO {}) {}",totals.get(text("id")).copied().unwrap_or(0),text("notes"))}else if let Some(parent)=row["parent_id"].as_str(){let position=positions.entry(parent.into()).or_default();*position+=1;format!("(SPLIT {} OF {}) {}",position,totals.get(parent).copied().unwrap_or(0),text("notes"))}else{text("notes").into()};fields.extend([string(&notes),string(group),string(category),if row["is_parent"]==true{"0".into()}else{amount(money)},if row["is_parent"]==true{amount(money)}else{"0".into()},if row["reconciled"]==true{"Reconciled"}else if row["cleared"]==true{"Cleared"}else{"Not cleared"}.into()]);}
   writer.write_record(fields).map_err(|e|Error::invalid(e.to_string()))?;
  }
  let bytes=writer.into_inner().map_err(|e|Error::invalid(e.to_string()))?.0;Ok(bytes)
 }).await?;
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (
                axum::http::header::CONTENT_DISPOSITION,
                "attachment; filename=\"transactions.csv\"",
            ),
            (axum::http::header::CACHE_CONTROL, "no-store"),
        ],
        bytes,
    ))
}
