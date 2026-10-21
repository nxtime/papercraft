//! Three-way merge of two edited copies of the same JSON file (a page), so two people or agents
//! working on one shared page keep each other's changes instead of the last writer winning.
//!
//! The merge is structural and schema-agnostic:
//! * objects merge key by key;
//! * arrays of objects that carry an `id` merge per id (added or deleted on either side is kept,
//!   an id both sides added with different content gets a fresh id for the remote one);
//! * arrays of plain numbers or strings (child lists) merge as ordered sets;
//! * when both sides changed the same scalar, the local one wins; `next_id` takes the larger.

use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Key of the counter that hands out new node ids.
const NEXT_ID: &str = "next_id";
/// Key of the node list and of the lists of child ids that point into it.
const NODES: &str = "nodes";
const CHILD_LISTS: [&str; 2] = ["children", "root"];

/// Merges `local` and `remote`, both edited from `base`.
pub fn merge(base: &Value, local: &Value, remote: &Value) -> Value {
    let mut remote = remote.clone();
    renumber_clashes(base, local, &mut remote);
    merge_value(Some(base), Some(local), Some(&remote)).unwrap_or(Value::Null)
}

fn merge_value(b: Option<&Value>, l: Option<&Value>, r: Option<&Value>) -> Option<Value> {
    if l == r {
        return l.cloned();
    }
    if l == b {
        return r.cloned();
    }
    if r == b {
        return l.cloned();
    }
    match (l, r) {
        (Some(Value::Object(lo)), Some(Value::Object(ro))) => {
            let empty = Map::new();
            let bo = b.and_then(Value::as_object).unwrap_or(&empty);
            Some(Value::Object(merge_objects(bo, lo, ro)))
        }
        (Some(Value::Array(la)), Some(Value::Array(ra))) => {
            let empty = Vec::new();
            let ba = b.and_then(Value::as_array).unwrap_or(&empty);
            Some(Value::Array(merge_arrays(ba, la, ra)))
        }
        _ => l.cloned(),
    }
}

fn merge_objects(
    b: &Map<String, Value>,
    l: &Map<String, Value>,
    r: &Map<String, Value>,
) -> Map<String, Value> {
    let keys: BTreeSet<&String> = b.keys().chain(l.keys()).chain(r.keys()).collect();
    let mut out = Map::new();
    for k in keys {
        let merged = if k == NEXT_ID {
            match (
                l.get(k).and_then(Value::as_u64),
                r.get(k).and_then(Value::as_u64),
            ) {
                (Some(a), Some(c)) => Some(Value::from(a.max(c))),
                _ => merge_value(b.get(k), l.get(k), r.get(k)),
            }
        } else {
            merge_value(b.get(k), l.get(k), r.get(k))
        };
        if let Some(v) = merged {
            out.insert(k.clone(), v);
        }
    }
    out
}

fn id_of(v: &Value) -> Option<String> {
    let id = v.as_object()?.get("id")?;
    id.as_u64()
        .map(|n| n.to_string())
        .or_else(|| id.as_str().map(str::to_owned))
}

fn keyed(a: &[Value]) -> Option<BTreeMap<String, &Value>> {
    a.iter().map(|v| id_of(v).map(|id| (id, v))).collect()
}

fn merge_arrays(b: &[Value], l: &[Value], r: &[Value]) -> Vec<Value> {
    if let (Some(bk), Some(lk), Some(rk)) = (keyed(b), keyed(l), keyed(r))
        && !(l.is_empty() && r.is_empty())
    {
        // Local order first, then what only the remote side added.
        let order: Vec<&String> = l
            .iter()
            .filter_map(|v| id_of(v).and_then(|id| lk.get_key_value(&id).map(|(k, _)| k)))
            .chain(r.iter().filter_map(|v| {
                let id = id_of(v)?;
                (!lk.contains_key(&id))
                    .then(|| rk.get_key_value(&id).map(|(k, _)| k))
                    .flatten()
            }))
            .collect();
        return order
            .into_iter()
            .filter_map(|id| {
                merge_value(
                    bk.get(id).copied(),
                    lk.get(id).copied(),
                    rk.get(id).copied(),
                )
            })
            .collect();
    }
    if l.iter().chain(r).all(|v| v.is_number() || v.is_string()) {
        let removed_remote = |x: &Value| b.contains(x) && !r.contains(x);
        let removed_local = |x: &Value| b.contains(x) && !l.contains(x);
        let mut out: Vec<Value> = l.iter().filter(|x| !removed_remote(x)).cloned().collect();
        out.extend(
            r.iter()
                .filter(|x| !l.contains(x) && !b.contains(x) && !removed_local(x))
                .cloned(),
        );
        return out;
    }
    l.to_vec()
}

/// Both sides handing out the same new node id for different nodes: the remote node (and the
/// child/root lists that mention it) move to a fresh id so both survive.
fn renumber_clashes(base: &Value, local: &Value, remote: &mut Value) {
    let ids = |v: &Value| -> BTreeMap<u64, Value> {
        v.get(NODES)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|n| Some((n.get("id")?.as_u64()?, n.clone())))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (base_ids, local_ids, remote_ids) = (ids(base), ids(local), ids(remote));
    let clashes: Vec<u64> = remote_ids
        .iter()
        .filter(|(id, n)| {
            !base_ids.contains_key(id) && local_ids.get(id).is_some_and(|ln| ln != *n)
        })
        .map(|(id, _)| *id)
        .collect();
    if clashes.is_empty() {
        return;
    }
    let next = [base, local, &*remote]
        .iter()
        .filter_map(|v| v.get(NEXT_ID).and_then(Value::as_u64))
        .chain(local_ids.keys().chain(remote_ids.keys()).map(|id| id + 1))
        .max()
        .unwrap_or(1);
    let map: BTreeMap<u64, u64> = clashes
        .iter()
        .enumerate()
        .map(|(i, id)| (*id, next + i as u64))
        .collect();
    let remap = |v: &mut Value| {
        if let Some(n) = v.as_u64().and_then(|n| map.get(&n)) {
            *v = Value::from(*n);
        }
    };
    if let Some(nodes) = remote.get_mut(NODES).and_then(Value::as_array_mut) {
        for node in nodes {
            if let Some(id) = node.get_mut("id") {
                remap(id);
            }
            for key in CHILD_LISTS {
                if let Some(list) = node.get_mut(key).and_then(Value::as_array_mut) {
                    list.iter_mut().for_each(remap);
                }
            }
        }
    }
    if let Some(list) = remote.get_mut("root").and_then(Value::as_array_mut) {
        list.iter_mut().for_each(remap);
    }
    if let Some(n) = remote.get_mut(NEXT_ID) {
        *n = Value::from(next + clashes.len() as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(nodes: Value, root: Value, next: u64) -> Value {
        json!({"format": "page", "next_id": next, "root": root, "nodes": nodes})
    }

    #[test]
    fn edits_to_different_nodes_both_survive() {
        let base = page(
            json!([{"id": 1, "name": "a", "x": 0}, {"id": 2, "name": "b", "x": 0}]),
            json!([1, 2]),
            3,
        );
        let local = page(
            json!([{"id": 1, "name": "a", "x": 5}, {"id": 2, "name": "b", "x": 0}]),
            json!([1, 2]),
            3,
        );
        let remote = page(
            json!([{"id": 1, "name": "a", "x": 0}, {"id": 2, "name": "B!", "x": 0}]),
            json!([1, 2]),
            3,
        );
        let m = merge(&base, &local, &remote);
        assert_eq!(m["nodes"][0]["x"], 5);
        assert_eq!(m["nodes"][1]["name"], "B!");
    }

    #[test]
    fn nodes_added_on_both_sides_are_kept_and_clashing_ids_are_renumbered() {
        let base = page(json!([]), json!([]), 1);
        let local = page(json!([{"id": 1, "name": "mine"}]), json!([1]), 2);
        let remote = page(json!([{"id": 1, "name": "theirs"}]), json!([1]), 2);
        let m = merge(&base, &local, &remote);
        let names: Vec<&str> = m["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["mine", "theirs"]);
        let ids: BTreeSet<u64> = m["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids.len(), 2, "ids stay unique");
        assert_eq!(m["root"].as_array().unwrap().len(), 2);
        assert!(m["next_id"].as_u64().unwrap() > *ids.iter().max().unwrap());
    }

    #[test]
    fn a_delete_on_one_side_wins_over_no_change_on_the_other() {
        let base = page(
            json!([{"id": 1, "name": "a"}, {"id": 2, "name": "b"}]),
            json!([1, 2]),
            3,
        );
        let local = page(json!([{"id": 1, "name": "a"}]), json!([1]), 3);
        let remote = base.clone();
        let m = merge(&base, &local, &remote);
        assert_eq!(m["nodes"].as_array().unwrap().len(), 1);
        assert_eq!(m["root"], json!([1]));
    }

    #[test]
    fn the_same_property_changed_on_both_sides_keeps_the_local_value() {
        let base = page(json!([{"id": 1, "x": 0}]), json!([1]), 2);
        let local = page(json!([{"id": 1, "x": 1}]), json!([1]), 2);
        let remote = page(json!([{"id": 1, "x": 2}]), json!([1]), 2);
        assert_eq!(merge(&base, &local, &remote)["nodes"][0]["x"], 1);
    }
}
