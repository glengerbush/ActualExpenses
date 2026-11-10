//! Ordered graph projection corresponding to pinned26.10 Sankey semantics.
//! Hidden -1 links are routing constraints, never financial contributions.
use crate::{error::Error, models::MAX_MONEY};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::Rc,
};
pub(super) const OTHER: &str = "__OTHER_BUCKET";
pub(super) const HIDDEN: &str = "__HIDDEN";
pub(super) const NEGATIVE: &str = "__NEGATIVE";
pub(super) const TO_BUDGET: &str = "to_budget";
pub(super) const BUDGETED: &str = "budgeted";
pub(super) const OVERSPENT: &str = "last_month_overspent";
pub(super) const NEXT: &str = "for_next_month";
pub(super) const PREVIOUS: &str = "from_previous_month";
pub(super) const AVAILABLE: &str = "available_income";
pub(super) const ALL_ACCOUNTS: &str = "all_income";
const STRUCTURAL: [&str; 11] = [
    TO_BUDGET,
    BUDGETED,
    OVERSPENT,
    NEXT,
    PREVIOUS,
    AVAILABLE,
    ALL_ACCOUNTS,
    OTHER,
    HIDDEN,
    NEGATIVE,
    "",
];
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(super) enum Layer {
    #[serde(rename = "payee")]
    Payee,
    #[serde(rename = "income_category")]
    IncomeCategory,
    #[serde(rename = "account")]
    Account,
    #[serde(rename = "budget")]
    Budget,
    #[serde(rename = "category_group")]
    Group,
    #[serde(rename = "category")]
    Category,
}
pub(super) const LAYERS: [Layer; 6] = [
    Layer::Payee,
    Layer::IncomeCategory,
    Layer::Account,
    Layer::Budget,
    Layer::Group,
    Layer::Category,
];
impl Layer {
    fn text(self) -> &'static str {
        match self {
            Self::Payee => "payee",
            Self::IncomeCategory => "income_category",
            Self::Account => "account",
            Self::Budget => "budget",
            Self::Group => "category_group",
            Self::Category => "category",
        }
    }
}
#[derive(Clone)]
pub(super) struct Node {
    pub kind: Layer,
    pub name: Option<String>,
    pub label: Option<String>,
    pub params: Option<Value>,
    pub negative: Option<bool>,
    pub to: Vec<(String, i128)>,
    percentage: Option<String>,
    measure: Option<i64>,
    color: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Source {
    Entry(usize),
    Transfer(usize),
    Context(String),
}
#[derive(Clone)]
pub(super) struct Annotation {
    pub source: Source,
    pub factor: i64,
}
#[derive(Clone, Default)]
pub(super) struct Graph {
    pub order: Vec<String>,
    pub nodes: BTreeMap<String, Node>,
    pub members: BTreeMap<(String, String), Vec<(String, String)>>,
    pub annotations: BTreeMap<(String, String), Vec<Annotation>>,
    active: Option<(Source, i128)>,
    budget: Rc<Budget>,
    too_large: bool,
}
pub(super) fn exact(value: i128) -> Result<i64, Error> {
    if value.abs() > MAX_MONEY as i128 {
        Err(Error::invalid("Sankey result exceeds exact monetary range"))
    } else {
        Ok(value as i64)
    }
}
const GRAPH_WORK: &str = "Sankey graph work exceeds50000000 charged units; select fewer categories/accounts or increase the visible-node cap";
/// One request-scoped budget covers base mutations, grouping, layout and metadata.
/// Cloned graphs share this counter, not mutable financial graph data.
struct Budget {
    remaining: Cell<u64>,
    failed: Cell<bool>,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            remaining: Cell::new(50_000_000),
            failed: Cell::new(false),
        }
    }
}
impl Budget {
    fn charge(&self, amount: usize) -> Result<(), Error> {
        if self.failed.get() {
            return Err(Error::invalid(GRAPH_WORK));
        }
        let Some(next) = self.remaining.get().checked_sub(amount as u64) else {
            self.failed.set(true);
            return Err(Error::invalid(GRAPH_WORK));
        };
        self.remaining.set(next);
        Ok(())
    }
}
pub(super) struct Work {
    budget: Rc<Budget>,
}
impl Work {
    fn shared(graph: &Graph) -> Self {
        Self {
            budget: graph.budget.clone(),
        }
    }
    pub fn charge(&mut self, amount: usize) -> Result<(), Error> {
        self.budget.charge(amount)
    }
}
impl Graph {
    pub fn add(&mut self, key: &str, kind: Layer, name: Option<&str>) {
        if self.too_large || self.budget.failed.get() || self.budget.charge(1).is_err() {
            return;
        }
        if !self.nodes.contains_key(key) {
            // Allow the source's small create-then-delete transition when an
            // Other bucket replaces a node exactly at the output cap.
            if self.nodes.len() >= 5006 {
                self.too_large = true;
                return;
            }
            self.order.push(key.into());
            self.nodes.insert(
                key.into(),
                Node {
                    kind,
                    name: name.map(str::to_owned),
                    label: None,
                    params: None,
                    negative: None,
                    to: vec![],
                    percentage: None,
                    measure: None,
                    color: None,
                },
            );
        }
    }
    pub fn labeled(
        &mut self,
        key: &str,
        kind: Layer,
        label: &str,
        params: Option<Value>,
        negative: Option<bool>,
    ) {
        if !self.nodes.contains_key(key) {
            self.add(key, kind, None);
            let Some(n) = self.nodes.get_mut(key) else {
                return;
            };
            n.label = Some(label.into());
            n.params = params;
            n.negative = negative;
        }
    }
    pub fn tag(&mut self, source: Option<(Source, i128)>) {
        self.active = source;
    }
    pub fn link(&mut self, from: &str, to: &str, value: i128) {
        if self.too_large || self.budget.failed.get() {
            return;
        }
        let cost = self.nodes.get(from).map_or(1, |node| node.to.len() + 1)
            + self
                .annotations
                .get(&(from.into(), to.into()))
                .map_or(0, Vec::len);
        if self.budget.charge(cost).is_err() {
            return;
        }
        if let Some(node) = self.nodes.get_mut(from) {
            if let Some((source, base)) = &self.active {
                let factor = if *base != 0 && value == -*base { -1 } else { 1 };
                let annotations = self
                    .annotations
                    .entry((from.into(), to.into()))
                    .or_default();
                if let Some(prior) = annotations
                    .iter_mut()
                    .find(|annotation| annotation.source == *source)
                {
                    prior.factor += factor;
                } else {
                    annotations.push(Annotation {
                        source: source.clone(),
                        factor,
                    });
                }
            }
            if let Some(edge) = node.to.iter_mut().find(|edge| edge.0 == to) {
                edge.1 += value;
            } else {
                node.to.push((to.into(), value));
            }
        }
    }
    pub(super) fn edge(&self, from: &str, to: &str) -> Option<i128> {
        self.nodes
            .get(from)?
            .to
            .iter()
            .find(|edge| edge.0 == to)
            .map(|edge| edge.1)
    }
    fn unlink(&mut self, from: &str, to: &str) {
        if let Some(node) = self.nodes.get_mut(from) {
            node.to.retain(|edge| edge.0 != to);
        }
    }
    fn delete(&mut self, key: &str) {
        self.nodes.remove(key);
        self.order.retain(|k| k != key);
    }
    fn edges(&self) -> usize {
        self.nodes.values().map(|n| n.to.len()).sum()
    }
    fn bounds(&self) -> Result<(), Error> {
        if self.budget.failed.get() {
            return Err(Error::invalid(GRAPH_WORK));
        }
        if self.too_large || self.nodes.len() > 5000 || self.edges() > 20000 {
            Err(Error::invalid(
                "Sankey permits5000 nodes and20000 links; narrow the selection",
            ))
        } else {
            Ok(())
        }
    }
    pub fn raw(&self) -> Result<Value, Error> {
        self.bounds()?;
        let mut output = vec![];
        for key in &self.order {
            let node = &self.nodes[key];
            let mut value = json!({"key":key,"type":node.kind,"to":node.to.iter().map(|(k,v)|Ok(json!([k,exact(*v)?]))).collect::<Result<Vec<_>,Error>>()?});
            let map = value.as_object_mut().unwrap();
            if let Some(v) = &node.name {
                map.insert("name".into(), json!(v));
            }
            if let Some(v) = &node.label {
                map.insert("labelKey".into(), json!(v));
            }
            if let Some(v) = &node.params {
                map.insert("labelParams".into(), v.clone());
            }
            if let Some(v) = node.negative {
                map.insert("isNegative".into(), json!(v));
            }
            output.push(value);
        }
        Ok(json!(output))
    }
    fn keys_in(&self, kind: Layer) -> Vec<String> {
        self.order
            .iter()
            .filter(|k| self.nodes[*k].kind == kind)
            .cloned()
            .collect()
    }
    fn has_parent(&self, key: &str) -> bool {
        self.nodes
            .values()
            .any(|node| node.to.iter().any(|edge| edge.0 == key))
    }
    fn begin_membership(&mut self) {
        for from in &self.order {
            for (to, _) in &self.nodes[from].to {
                self.members
                    .insert((from.clone(), to.clone()), vec![(from.clone(), to.clone())]);
            }
        }
    }
    fn merge_members(&mut self, from: &str, to: &str, original_from: &str, original_to: &str) {
        let original = self
            .members
            .get(&(original_from.into(), original_to.into()))
            .cloned()
            .unwrap_or_default();
        let members = self.members.entry((from.into(), to.into())).or_default();
        if self
            .budget
            .charge(
                original
                    .len()
                    .saturating_mul(members.len() + original.len()),
            )
            .is_err()
        {
            return;
        }
        for pair in original {
            if !members.contains(&pair) {
                members.push(pair);
            }
        }
    }
}
/// Iterative strongly-connected components and longest paths in their DAG.
/// This has the same layers as the source Tarjan calculation, without recursive
/// stack growth on a long graph supplied by imported categories/accounts.
fn layers(graph: &Graph, work: &mut Work) -> Result<BTreeMap<String, usize>, Error> {
    work.charge(graph.nodes.len() + graph.edges())?;
    let mut seen = BTreeSet::new();
    let mut finished = vec![];
    for root in &graph.order {
        if seen.contains(root) {
            continue;
        }
        let mut stack = vec![(root.clone(), false)];
        while let Some((key, exit)) = stack.pop() {
            if exit {
                finished.push(key);
                continue;
            }
            if !seen.insert(key.clone()) {
                continue;
            }
            stack.push((key.clone(), true));
            for (child, _) in graph.nodes[&key].to.iter().rev() {
                if graph.nodes.contains_key(child) && !seen.contains(child) {
                    stack.push((child.clone(), false));
                }
            }
        }
    }
    let mut reverse = BTreeMap::<String, Vec<String>>::new();
    for from in &graph.order {
        for (to, _) in &graph.nodes[from].to {
            if graph.nodes.contains_key(to) {
                reverse.entry(to.clone()).or_default().push(from.clone());
            }
        }
    }
    let mut component = BTreeMap::new();
    let mut count = 0;
    for root in finished.iter().rev() {
        if component.contains_key(root) {
            continue;
        }
        let mut stack = vec![root.clone()];
        while let Some(key) = stack.pop() {
            if component.contains_key(&key) {
                continue;
            }
            component.insert(key.clone(), count);
            for parent in reverse.get(&key).into_iter().flatten() {
                if !component.contains_key(parent) {
                    stack.push(parent.clone());
                }
            }
        }
        count += 1;
    }
    let mut outgoing = vec![BTreeSet::new(); count];
    let mut incoming = vec![0; count];
    for from in &graph.order {
        for (to, _) in &graph.nodes[from].to {
            if let Some(&target) = component.get(to) {
                let source = component[from];
                if source != target && outgoing[source].insert(target) {
                    incoming[target] += 1;
                }
            }
        }
    }
    let mut queue = (0..count)
        .filter(|&i| incoming[i] == 0)
        .collect::<VecDeque<_>>();
    let mut values = vec![0; count];
    while let Some(source) = queue.pop_front() {
        for &target in &outgoing[source] {
            values[target] = values[target].max(values[source] + 1);
            incoming[target] -= 1;
            if incoming[target] == 0 {
                queue.push_back(target);
            }
        }
    }
    Ok(component
        .into_iter()
        .map(|(key, index)| (key, values[index]))
        .collect())
}
/// One incoming/outgoing sweep preserves the source node-value rule while
/// avoiding a complete edge scan per node during every Other merge.
fn node_values(
    graph: &Graph,
    computed: &BTreeMap<String, usize>,
    work: &mut Work,
) -> Result<BTreeMap<String, i128>, Error> {
    work.charge(graph.nodes.len() + graph.edges())?;
    let mut incoming = BTreeMap::<String, i128>::new();
    let mut outgoing = BTreeMap::<String, i128>::new();
    for source in &graph.order {
        for (target, value) in &graph.nodes[source].to {
            if !target.ends_with(HIDDEN) {
                *outgoing.entry(source.clone()).or_default() += value;
            }
            if !source.ends_with(HIDDEN) {
                *incoming.entry(target.clone()).or_default() += value;
            }
        }
    }
    Ok(graph
        .order
        .iter()
        .map(|key| {
            let received = incoming.get(key).copied().unwrap_or(0);
            let value = if computed.get(key).copied().unwrap_or(0) == 0 || received < 0 {
                outgoing.get(key).copied().unwrap_or(0)
            } else {
                received
            };
            (key.clone(), value)
        })
        .collect())
}
#[derive(Clone, Serialize)]
pub(super) struct Tooltip {
    pub name: String,
    pub value: i64,
}
type Tooltips = BTreeMap<(String, String), Vec<Tooltip>>;
fn tooltip(
    tips: &mut Tooltips,
    from: &str,
    to: &str,
    name: &str,
    value: i128,
    work: &mut Work,
) -> Result<(), Error> {
    work.charge(
        tips.values().map(Vec::len).sum::<usize>()
            + tips.get(&(from.into(), to.into())).map_or(0, Vec::len)
            + 1,
    )?;
    let entries = tips.entry((from.into(), to.into())).or_default();
    if !entries.iter().any(|entry| entry.name == name) {
        entries.push(Tooltip {
            name: name.into(),
            value: exact(value)?,
        });
    }
    if tips.values().map(Vec::len).sum::<usize>() > 20000 {
        return Err(Error::invalid(
            "Sankey grouped detail exceeds20000 cells; increase visible nodes or narrow the selection",
        ));
    }
    Ok(())
}
fn move_other(
    graph: &mut Graph,
    key: &str,
    tips: &mut Tooltips,
    global: bool,
    work: &mut Work,
) -> Result<(), Error> {
    work.charge(graph.nodes.len() + graph.edges())?;
    let node = graph.nodes[key].clone();
    let mut other = format!("{}{}", node.kind.text(), OTHER);
    if !global {
        if node.kind == Layer::Category {
            if let Some(parent) = graph.order.iter().find(|from| {
                graph.nodes[*from].kind == Layer::Group && graph.edge(from, key).is_some()
            }) {
                other = format!("{parent}{OTHER}");
            }
        } else if node.kind == Layer::Payee
            && let Some((target, _)) = node.to.iter().find(|(target, _)| {
                graph
                    .nodes
                    .get(target)
                    .is_some_and(|n| n.kind == Layer::IncomeCategory)
            })
        {
            other = format!("{target}{OTHER}");
        }
    }
    let from = graph
        .order
        .iter()
        .filter_map(|source| graph.edge(source, key).map(|value| (source.clone(), value)))
        .collect::<Vec<_>>();
    graph.add(&other, node.kind, Some("Other"));
    let name = node.name.as_deref().unwrap_or(key);
    for (source, value) in &from {
        if source != &other {
            graph.link(source, &other, *value);
            graph.merge_members(source, &other, source, key);
            tooltip(tips, source, &other, name, *value, work)?;
        }
    }
    for (target, value) in &node.to {
        if target != &other {
            graph.link(&other, target, *value);
            graph.merge_members(&other, target, key, target);
            tooltip(tips, &other, target, name, *value, work)?;
        }
    }
    for (target, _) in &node.to {
        graph.unlink(key, target);
    }
    for (source, _) in from {
        graph.unlink(&source, key);
    }
    graph.delete(key);
    graph.bounds()
}
fn group_other(
    graph: &mut Graph,
    top: usize,
    sort: &str,
    work: &mut Work,
) -> Result<Tooltips, Error> {
    let mut tips = Tooltips::new();
    for kind in LAYERS {
        loop {
            let keys = graph.keys_in(kind);
            let ordinary = keys
                .iter()
                .filter(|key| {
                    !key.ends_with(OTHER)
                        && !STRUCTURAL.contains(&key.as_str())
                        && graph.nodes[*key].negative != Some(true)
                })
                .collect::<Vec<_>>();
            let other = keys.iter().filter(|key| key.ends_with(OTHER)).count();
            if ordinary.len() + other <= top {
                break;
            }
            let computed = layers(graph, work)?;
            let values = node_values(graph, &computed, work)?;
            let mut smallest: Option<(String, i128)> = None;
            for key in ordinary {
                let value = values[key];
                if smallest
                    .as_ref()
                    .is_none_or(|(_, minimum)| value < *minimum)
                {
                    smallest = Some((key.clone(), value));
                }
            }
            let Some((key, _)) = smallest else {
                break;
            };
            move_other(graph, &key, &mut tips, sort == "global", work)?;
        }
    }
    Ok(tips)
}
fn move_end(order: &mut Vec<String>, key: &str, start: bool) {
    if let Some(index) = order.iter().position(|k| k == key) {
        let key = order.remove(index);
        if start {
            order.insert(0, key);
        } else {
            order.push(key);
        }
    }
}
fn sort_graph(
    graph: &mut Graph,
    sort: &str,
    categories: &[(String, Vec<String>)],
    work: &mut Work,
) -> Result<(), Error> {
    let computed = layers(graph, work)?;
    let values = node_values(graph, &computed, work)?;
    let mut order = graph.order.clone();
    if sort == "global" || sort == "per-group" {
        order.sort_by(|a, b| values[b].cmp(&values[a]));
        if sort == "global" {
            for index in 0..order.len() {
                let key = order[index].clone();
                if key.ends_with(OTHER) {
                    work.charge(order.len())?;
                    move_end(&mut order, &key, false);
                }
            }
        } else {
            for (anchor_kind, related_kind, before) in [
                (Layer::Group, Layer::Category, false),
                (Layer::IncomeCategory, Layer::Payee, true),
            ] {
                for anchor in graph.keys_in(anchor_kind) {
                    let related = if before {
                        graph
                            .order
                            .iter()
                            .filter(|key| {
                                graph.nodes[*key].kind == related_kind
                                    && graph.edge(key, &anchor).is_some()
                            })
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        graph.nodes[&anchor]
                            .to
                            .iter()
                            .filter(|(key, _)| {
                                graph
                                    .nodes
                                    .get(key)
                                    .is_some_and(|node| node.kind == related_kind)
                            })
                            .map(|(key, _)| key.clone())
                            .collect()
                    };
                    let other = format!("{anchor}{OTHER}");
                    let related_other = graph
                        .nodes
                        .get(&other)
                        .is_some_and(|n| n.kind == related_kind);
                    let mut ordered = related
                        .into_iter()
                        .filter(|key| key != &other && order.contains(key))
                        .collect::<Vec<_>>();
                    ordered.sort_by(|a, b| values[b].cmp(&values[a]));
                    let mut remove = ordered.iter().cloned().collect::<BTreeSet<_>>();
                    if related_other {
                        remove.insert(other.clone());
                    }
                    let mut remaining = order
                        .iter()
                        .filter(|key| !remove.contains(*key))
                        .cloned()
                        .collect::<Vec<_>>();
                    if let Some(index) = remaining.iter().position(|key| key == &anchor) {
                        if related_other && order.contains(&other) {
                            ordered.push(other);
                        }
                        remaining.splice(
                            index + usize::from(!before)..index + usize::from(!before),
                            ordered,
                        );
                        order = remaining;
                    }
                }
            }
        }
    } else {
        order.clear();
        let mut used = BTreeSet::new();
        for (group, cats) in categories {
            for key in std::iter::once(group.clone())
                .chain(cats.iter().cloned())
                .chain(std::iter::once(format!("{group}{OTHER}")))
            {
                if graph.nodes.contains_key(&key) {
                    order.push(key.clone());
                    used.insert(key);
                }
            }
        }
        for key in &graph.order {
            if !used.contains(key) {
                order.push(key.clone());
            }
        }
    }
    let negative = order
        .iter()
        .filter(|key| graph.nodes[*key].negative == Some(true))
        .cloned()
        .collect::<Vec<_>>();
    work.charge(order.len().saturating_mul(negative.len() + 4))?;
    for key in negative {
        move_end(&mut order, &key, true);
    }
    if let Some(node) = graph.nodes.get(TO_BUDGET) {
        move_end(&mut order, TO_BUDGET, node.negative == Some(true));
    }
    for key in [OVERSPENT, NEXT, PREVIOUS] {
        move_end(&mut order, key, false);
    }
    graph.order = order;
    Ok(())
}
fn cleanup(graph: &mut Graph) {
    for node in graph.nodes.values_mut() {
        node.to.retain(|(_, value)| *value != 0);
    }
    let incoming = graph
        .nodes
        .values()
        .flat_map(|node| node.to.iter().map(|(key, _)| key.clone()))
        .collect::<BTreeSet<_>>();
    let remove = graph
        .order
        .iter()
        .filter(|key| graph.nodes[*key].to.is_empty() && !incoming.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    for key in remove {
        graph.delete(&key);
    }
}
fn hidden_nodes(graph: &mut Graph, work: &mut Work) -> Result<(), Error> {
    let mut types: Vec<(Layer, Vec<String>, bool, bool)> = vec![];
    for key in &graph.order {
        let kind = graph.nodes[key].kind;
        if let Some((_, nodes, _, _)) = types.iter_mut().find(|(k, _, _, _)| *k == kind) {
            nodes.push(key.clone());
        } else {
            types.push((kind, vec![key.clone()], false, false));
        }
    }
    for (_, keys, parent, child) in &mut types {
        for key in keys {
            work.charge(graph.nodes.len() + graph.edges())?;
            *parent |= graph.has_parent(key);
            *child |= !graph.nodes[key].to.is_empty();
        }
    }
    let active = LAYERS
        .into_iter()
        .filter(|kind| types.iter().any(|(k, _, _, _)| k == kind))
        .collect::<Vec<_>>();
    for (kind, keys, type_parent, type_child) in types {
        let index = active.iter().position(|k| *k == kind).unwrap();
        for key in keys {
            work.charge(graph.nodes.len() + graph.edges())?;
            let parent = graph.has_parent(&key);
            let child = !graph.nodes[&key].to.is_empty();
            if !parent && type_parent {
                let mut child = key.clone();
                for kind in active[..index].iter().rev() {
                    let parent = format!("{}_{}{}", key, kind.text(), HIDDEN);
                    graph.add(&parent, *kind, Some(""));
                    graph.link(&parent, &child, -1);
                    child = parent;
                }
            }
            if !child && type_child {
                let mut parent = key.clone();
                for kind in &active[index + 1..] {
                    let child = format!("{}_{}{}", key, kind.text(), HIDDEN);
                    graph.add(&child, *kind, Some(""));
                    graph.link(&parent, &child, -1);
                    parent = child;
                }
            }
            graph.bounds()?;
        }
    }
    let mut computed = layers(graph, work)?;
    let max_account = graph
        .order
        .iter()
        .filter(|key| graph.nodes[*key].kind == Layer::Account && !key.ends_with(HIDDEN))
        .filter_map(|key| computed.get(key))
        .copied()
        .max();
    let Some(max_account) = max_account else {
        return Ok(());
    };
    let push = graph
        .order
        .iter()
        .filter(|key| {
            graph.nodes[*key].kind == Layer::Group
                && computed
                    .get(*key)
                    .is_some_and(|layer| *layer <= max_account)
        })
        .cloned()
        .collect::<Vec<_>>();
    for key in push {
        while computed
            .get(&key)
            .is_some_and(|layer| *layer <= max_account)
        {
            work.charge(graph.nodes.len() + graph.edges())?;
            let mut deepest: Option<(String, usize)> = None;
            for parent in &graph.order {
                if graph.edge(parent, &key).is_some()
                    && let Some(&layer) = computed.get(parent)
                    && deepest.as_ref().is_none_or(|(_, old)| layer > *old)
                {
                    deepest = Some((parent.clone(), layer));
                }
            }
            let Some((parent, _)) = deepest else {
                break;
            };
            let hidden = format!("{key}_account_{}{HIDDEN}", computed[&key]);
            graph.add(&hidden, Layer::Account, Some(""));
            graph.link(&parent, &hidden, -1);
            graph.link(&hidden, &key, -1);
            graph.bounds()?;
            computed = layers(graph, work)?;
        }
    }
    Ok(())
}
/// ECMAScript toFixed(1) rounds the exact binary float halfway upward; Rust's
/// decimal formatter alone rounds ties to even. Decode this nonnegative bounded
/// percentage so source labels agree at both exact and inexact decimal ties.
fn percentage(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return "0.0%".into();
    }
    let negative = value < 0.0;
    let bits = value.abs().to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    let mantissa = ((bits & ((1u64 << 52) - 1)) | (1u64 << 52)) as u128;
    let numerator = mantissa * 10;
    let rounded = if exponent >= 0 {
        numerator.checked_shl(exponent as u32).unwrap_or(0)
    } else {
        let shift = (-exponent) as u32;
        if shift >= 128 {
            0
        } else {
            let denominator = 1u128 << shift;
            let quotient = numerator / denominator;
            let remainder = numerator % denominator;
            quotient + u128::from(remainder >= denominator / 2)
        }
    };
    format!(
        "{}{}.{:01}%",
        if negative { "-" } else { "" },
        rounded / 10,
        rounded % 10
    )
}
fn add_percentages(graph: &mut Graph, work: &mut Work) -> Result<(), Error> {
    let computed = layers(graph, work)?;
    let mut totals = BTreeMap::<usize, i128>::new();
    let values = node_values(graph, &computed, work)?;
    for key in &graph.order {
        if key.ends_with(HIDDEN) {
            continue;
        }
        if let Some(layer) = computed.get(key) {
            let value = values[key];
            exact(value)?;
            *totals.entry(*layer).or_default() += value;
        }
    }
    for key in &graph.order {
        let node = graph.nodes.get_mut(key).unwrap();
        node.measure = values.get(key).map(|value| exact(*value)).transpose()?;
        node.percentage = if key.ends_with(HIDDEN) {
            None
        } else {
            computed.get(key).map(|layer| {
                let total = totals.get(layer).copied().unwrap_or(1);
                percentage(if total == 0 {
                    0.0
                } else {
                    values.get(key).copied().unwrap_or(0) as f64 / total as f64 * 100.0
                })
            })
        };
    }
    Ok(())
}
fn colors(graph: &mut Graph) {
    for key in &graph.order {
        let hex = key
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(3)
            .collect::<String>();
        let value = usize::from_str_radix(if hex.is_empty() { "0" } else { &hex }, 16).unwrap_or(0);
        graph.nodes.get_mut(key).unwrap().color = Some(format!("palette{}", value % 3));
    }
    let overrides = [
        (TO_BUDGET, "positive"),
        (OVERSPENT, "negative"),
        (PREVIOUS, "neutral"),
        (NEXT, "neutral"),
        (BUDGETED, "flow"),
        (AVAILABLE, "flow"),
        (ALL_ACCOUNTS, "flow"),
    ];
    for (key, color) in overrides {
        if let Some(node) = graph.nodes.get_mut(key) {
            node.color = Some(color.into());
        }
    }
    for node in graph.nodes.values_mut() {
        if node.negative == Some(true) {
            node.color = Some("negative".into());
        }
    }
}
fn label(node: &Node, key: &str) -> String {
    if let Some(label) = &node.label {
        let mut label = label.clone();
        if let Some(params) = node.params.as_ref().and_then(Value::as_object) {
            for (name, value) in params {
                if let Some(value) = value.as_str() {
                    label = label.replace(&format!("{{{{{name}}}}}"), value);
                }
            }
        }
        label
    } else {
        node.name.clone().unwrap_or_else(|| key.into())
    }
}
pub(super) struct Projection {
    pub result: Value,
    pub graph: Graph,
}
pub(super) fn build(
    base: &Graph,
    top: usize,
    categories: &[(String, Vec<String>)],
    sort: &str,
    from: Layer,
    to: Layer,
) -> Result<Projection, Error> {
    let mut work = Work::shared(base);
    let mut graph = base.clone();
    graph.bounds()?;
    graph.begin_membership();
    let tips = group_other(&mut graph, top, sort, &mut work)?;
    sort_graph(&mut graph, sort, categories, &mut work)?;
    cleanup(&mut graph);
    hidden_nodes(&mut graph, &mut work)?;
    add_percentages(&mut graph, &mut work)?;
    colors(&mut graph);
    let remove = graph
        .order
        .iter()
        .filter(|key| graph.nodes[*key].kind < from || graph.nodes[*key].kind > to)
        .cloned()
        .collect::<Vec<_>>();
    for key in remove {
        graph.delete(&key);
    }
    graph.bounds()?;
    let nodes=graph.order.iter().map(|key|{let node=&graph.nodes[key];json!({"key":key,"name":label(node,key),"percentageLabel":node.percentage.as_deref().unwrap_or(""),"color":node.color})}).collect::<Vec<_>>();
    let mut links = vec![];
    for (source, key) in graph.order.iter().enumerate() {
        let node = &graph.nodes[key];
        for (target, value) in &node.to {
            let mut info = tips
                .get(&(key.clone(), target.clone()))
                .cloned()
                .unwrap_or_default();
            info.sort_by_key(|entry| std::cmp::Reverse(entry.value));
            let mut color = if [
                Layer::Payee,
                Layer::IncomeCategory,
                Layer::Account,
                Layer::Group,
            ]
            .contains(&node.kind)
            {
                node.color.clone()
            } else if [Layer::Category, Layer::Budget].contains(&node.kind) {
                graph.nodes.get(target).and_then(|n| n.color.clone())
            } else {
                None
            };
            if target == OVERSPENT
                || target.ends_with(NEGATIVE)
                || target == TO_BUDGET
                || target == NEXT
            {
                color = graph.nodes.get(target).and_then(|n| n.color.clone());
            }
            if node.negative == Some(true) {
                color = node.color.clone();
            }
            let mut link = json!({"source":source,"target":graph.order.iter().position(|key|key==target).map(|index|index as i64).unwrap_or(-1),"value":exact(*value)?,"tooltipInfo":info});
            if let Some(color) = color {
                link.as_object_mut()
                    .unwrap()
                    .insert("color".into(), json!(color));
            }
            links.push(link);
        }
    }
    Ok(Projection {
        result: json!({"nodes":nodes,"links":links}),
        graph,
    })
}

/// Rendering and accessibility metadata leaves the source result unchanged.
/// Flow amounts/percentages retain complete context before layer restriction.
pub(super) fn metadata(graph: &Graph) -> Result<(Value, Value), Error> {
    let computed = layers(graph, &mut Work::shared(graph))?;
    let mut nodes = vec![];
    let mut links = vec![];
    for key in &graph.order {
        let node = &graph.nodes[key];
        let hidden = key.ends_with(HIDDEN);
        let owned = !(hidden
            || key.ends_with(OTHER)
            || STRUCTURAL.contains(&key.as_str())
            || node.label.as_deref() == Some("From {{category}}"));
        let label_key = if owned || hidden {
            None
        } else {
            node.label.clone().or_else(|| node.name.clone())
        };
        nodes.push(json!({"key":key,"layer":node.kind,"depth":computed.get(key).copied().unwrap_or(0),"hidden":hidden,"amount":node.measure,"label_key":label_key,"label_params":node.params,"user_owned":owned}));
        for (target, _) in &node.to {
            let routing = hidden || target.ends_with(HIDDEN);
            let outside = !graph.nodes.contains_key(target);
            let members = graph.members.get(&(key.clone(), target.clone()));
            links.push(json!({"source":key,"target":target,"kind":if routing{"routing"}else if outside{"outside_selection"}else{"flow"},"member_count":members.map_or(0,Vec::len)}));
        }
    }
    Ok((json!(nodes), json!(links)))
}
