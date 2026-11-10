//! Versioned bounded Sankey projections over immutable current-ledger reads.
mod data;
mod details;
mod graph;
use super::*;
use axum::Extension;
use graph::Layer;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u8,
    as_of: String,
    options: Options,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Options {
    start_date: String,
    end_date: String,
    #[serde(default = "spent")]
    mode: String,
    #[serde(default)]
    conditions: Vec<Value>,
    #[serde(default = "and")]
    conditions_op: String,
    #[serde(default)]
    group_accounts: bool,
    #[serde(default)]
    show_transfers: bool,
    #[serde(default = "top")]
    top_ncategories: usize,
    #[serde(default = "sort")]
    category_sort: String,
    #[serde(default)]
    layer_from: Option<Layer>,
    #[serde(default)]
    layer_to: Option<Layer>,
    #[serde(default)]
    show_percentages: bool,
}
fn spent() -> String {
    "spent".into()
}
fn and() -> String {
    "and".into()
}
fn sort() -> String {
    "per-group".into()
}
fn top() -> usize {
    15
}
type Cache = Arc<Mutex<super::revision_cache::Cache>>;
pub(crate) fn router() -> Router<Store> {
    Router::new()
        .route("/api/v1/budgets/{budget}/reports/sankey", post(read))
        .route(
            "/api/v1/budgets/{budget}/reports/sankey/drilldown",
            post(details::read),
        )
        .layer(Extension(Cache::default()))
}
fn layer_range(options: &Options) -> (Layer, Layer) {
    (
        options.layer_from.unwrap_or(if options.mode == "budgeted" {
            Layer::IncomeCategory
        } else {
            Layer::Payee
        }),
        options.layer_to.unwrap_or(Layer::Category),
    )
}
fn validate(query: &Request) -> Result<(), Error> {
    if query.version != 1 {
        return Err(Error::invalid("Unsupported Sankey version"));
    }
    day(&query.as_of)?;
    day(&query.options.start_date)?;
    day(&query.options.end_date)?;
    let options = &query.options;
    let start = crate::budgeting::month(&options.start_date[..7])?;
    let end = crate::budgeting::month(&options.end_date[..7])?;
    if start > end || end - start + 1 > 20000 {
        return Err(Error::invalid(
            "Sankey requires an ordered range up to20000 monthly intervals",
        ));
    }
    if options.mode == "budgeted" {
        crate::budgeting::month_name(end + 1)?;
    }
    if !matches!(options.mode.as_str(), "spent" | "budgeted")
        || !matches!(options.conditions_op.as_str(), "and" | "or")
        || !matches!(
            options.category_sort.as_str(),
            "per-group" | "global" | "budget-order"
        )
        || !(1..=100000).contains(&options.top_ncategories)
        || options.conditions.len() > 20
    {
        return Err(Error::invalid(
            "Choose a supported Sankey mode, sort,1–100000 nodes and up to20 and/or conditions",
        ));
    }
    for condition in &options.conditions {
        if !condition.is_object()
            || !condition["field"].is_string()
            || !condition["op"].is_string()
            || condition.get("value").is_none()
        {
            return Err(Error::invalid(
                "Sankey conditions require field/operator/value",
            ));
        }
    }
    let (from, to) = layer_range(options);
    if from >= to
        || options.mode == "spent" && (from == Layer::Budget || to == Layer::Budget)
        || options.mode == "budgeted" && (from == Layer::Payee || to == Layer::Payee)
    {
        return Err(Error::invalid(
            "Choose distinct ordered layers available in this Sankey mode",
        ));
    }
    if serde_json::to_vec(query).map_err(Error::internal)?.len() > 65536 {
        return Err(Error::invalid("Sankey query exceeds64KiB"));
    }
    Ok(())
}
fn revision(db: &Connection, b: &str, mode: &str) -> Result<String, Error> {
    let epoch = crate::lifecycle::epoch(db, b)?;
    let sequence:i64=db.query_row("SELECT COALESCE(max(sequence),0) FROM budget_changes WHERE budget_id=?1 AND entity IN ('transactions','accounts','payees','categories','category_groups')",[b],|r|r.get(0))?;
    let mut hash = Sha256::new();
    if mode == "budgeted" {
        let mut statement=db.prepare("SELECT entity,entity_id,field,clock FROM field_versions WHERE budget_id=?1 AND ((entity='budget_cells' AND field IN ('amount','carryover','tombstone')) OR (entity='budget_months' AND field IN ('buffered','tombstone')) OR (entity='budget_settings' AND field='method')) ORDER BY entity,entity_id,field")?;
        let mut rows = statement.query([b])?;
        while let Some(row) = rows.next()? {
            for column in 0..4 {
                let text: String = row.get(column)?;
                hash.update((text.len() as u64).to_le_bytes());
                hash.update(text.as_bytes());
            }
        }
    }
    Ok(format!("s1:{epoch}:{sequence:016x}:{:x}", hash.finalize()))
}

struct Evaluation {
    base: data::Base,
    graph: graph::Graph,
    projection: graph::Projection,
    hit: bool,
    revision: String,
}
fn evaluate(db: &Connection, b: &str, query: &Request, cache: &Cache) -> Result<Evaluation, Error> {
    store::require_budget(db, b)?;
    let preferences = crate::preferences::get_preferences(db, b)?;
    if preferences["experimental"]["sankeyReport"] != true {
        return Err(Error::new(
            StatusCode::CONFLICT,
            "feature_disabled",
            "Enable Sankey report in experimental settings before calculating it",
        ));
    }
    let options = &query.options;
    let revision = revision(db, b, &options.mode)?;
    let signature = json!({"start":&options.start_date[..7],"end":&options.end_date[..7],"mode":options.mode,"conditions":options.conditions,"conditionsOp":options.conditions_op,"showTransfers":options.mode=="spent"&&options.show_transfers});
    let key = format!("{b}:{revision}:{signature}");
    let prior = cache.lock().map_err(Error::internal)?.get(&key);
    let hit = prior.is_some();
    let base: data::Base = if let Some(prior) = prior {
        serde_json::from_value(prior).map_err(Error::internal)?
    } else {
        let base = data::read(db, b, options)?;
        cache
            .lock()
            .map_err(Error::internal)?
            .put(key, serde_json::to_value(&base).map_err(Error::internal)?);
        base
    };
    let graph = data::graph(&base, options.group_accounts)?;
    let order = base
        .groups
        .iter()
        .map(|group| {
            (
                group.id.clone(),
                group
                    .categories
                    .iter()
                    .map(|category| category.id.clone())
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let (from, to) = layer_range(options);
    let projection = graph::build(
        &graph,
        options.top_ncategories,
        &order,
        &options.category_sort,
        from,
        to,
    )?;
    Ok(Evaluation {
        base,
        graph,
        projection,
        hit,
        revision,
    })
}
fn response(
    db: &Connection,
    b: &str,
    query: &Request,
    evaluation: Evaluation,
) -> Result<Value, Error> {
    let (node_metadata, link_metadata) = graph::metadata(&evaluation.projection.graph)?;
    let method: String =
        db.query_row("SELECT budget_type FROM budgets WHERE id=?1", [b], |row| {
            row.get(0)
        })?;
    let value = json!({
     "method":method,"node_metadata":node_metadata,"link_metadata":link_metadata,
     "result":evaluation.projection.result,"base_graph":evaluation.graph.raw()?,
     "epoch":crate::lifecycle::epoch(db,b)?,"snapshot":sync::cursor(db,b)?,"as_of":query.as_of,
     "mode":query.options.mode,"recorded":query.options.mode=="spent",
     "basis":if query.options.mode=="spent"{"grouped_recorded_entries"}else{"budget_allocations_and_context"},
     "cache":{"hit":evaluation.hit,"input_revision":evaluation.revision},
     "assumptions":[
      {"code":"graph_conventions","message":"Flow layers repeat money; their links must not be added into one total. Refunds and negative allocations follow the source graph structure and can differ from budget summaries."},
      {"code":"routing","message":"Hidden routing links are layout constraints, not monetary entries. Outgoing links beyond the selected ending layer are retained for source comparison and omitted from the displayed selection."},
      {"code":"categories","message":"Only grouped categories appear. Uncategorized and ungrouped entries are not shown; review them in the account register."},
      {"code":"budgeted_basis","message":"Budgeted mode combines planned allocations with recorded income, carry-in, overspending and funds held forward. Its category filters do not filter the complete budget context."}
     ]
    });
    if value.to_string().len() > 8 * 1024 * 1024 {
        return Err(Error::invalid(
            "Sankey response exceeds8MiB; narrow the selection or show fewer nodes",
        ));
    }
    Ok(value)
}
async fn read(
    State(store): State<Store>,
    Path(b): Path<String>,
    Extension(cache): Extension<Cache>,
    input: Result<Json<Request>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    crate::models::validate_id(&b)?;
    let Json(query) = input.map_err(crate::api::json_error)?;
    validate(&query)?;
    let value = store
        .run(move |db| {
            let tx = db.transaction()?;
            super::sql_work::run(&tx, || {
                crate::register::with_report_predicates(&tx, || {
                    let evaluation = evaluate(&tx, &b, &query, &cache)?;
                    response(&tx, &b, &query, evaluation)
                })
            })
        })
        .await?;
    Ok(Json(json!({"data":value})))
}
