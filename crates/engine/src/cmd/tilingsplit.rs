//! Vetorial: Tiling's Split Elements (Inkscape's Tiling `split_items`). With `split` on, the object
//! keeps its first tile and the others are separate objects ("satellites", listed in the
//! effect's `satellites`) right above it, which can be selected, coloured and moved on their own.
//! They follow the object: whenever it changes (its shape, place, colours or the Tiling values),
//! they are made again from it. With `linkStyles` they take its colours and style each time;
//! without, each keeps its own once made. Turning `split` off or hiding the effect removes them;
//! so do removing the effect ([`erase`]) and deleting the object. Expanding the object's
//! appearance leaves them as ordinary objects.

use std::sync::Arc;

use serde_json::{Value, json};
use vectorcraft_doc::{Document, Effect, Node, NodeId};
use vectorcraft_geom::Affine;
use vectorcraft_render::effects;

/// The Tiling effect.
const EFFECT: &str = "distort.tiling";

/// The satellites Tiling effect `e` lists.
pub(crate) fn satellites_of(e: &Effect) -> Vec<NodeId> {
    e.params.get("satellites").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(NodeId).collect()).unwrap_or_default()
}

/// `n`'s first Tiling (among its own effects) and its position.
fn tiling_of(n: &Node) -> Option<(usize, &Effect)> {
    n.appearance.effects.iter().enumerate().find(|(_, e)| e.id == EFFECT)
}

/// Remove the nodes `ids` still in `doc` (skipping those inside another of them).
fn delete(doc: &mut Document, ids: &[NodeId]) {
    for id in ids {
        if doc.node(*id).is_some() {
            let _ = doc.remove(*id);
        }
    }
}

/// Remove the satellites of the split Tiling effects among `removed` (effects just taken off an
/// object): Remove Effect takes them away too.
pub(crate) fn erase(doc: &mut Document, removed: &[Effect]) {
    let ids: Vec<NodeId> = removed.iter().filter(|e| e.id == EFFECT).flat_map(satellites_of).collect();
    delete(doc, &ids);
}

/// Keep the split tiles of `doc` in step with their objects after an edit (`before` the document
/// before it).
pub(crate) fn refresh(before: &Document, doc: &mut Document) {
    // The objects with Tiling now, and those that had split tiles before.
    let mut now: Vec<NodeId> = vec![];
    doc.walk(|n| {
        if tiling_of(n).is_some() {
            now.push(n.id);
        }
    });
    let mut gone: Vec<NodeId> = vec![];
    before.walk(|n| {
        if let Some((_, e)) = tiling_of(n)
            && doc.node(n.id).is_none()
        {
            gone.extend(satellites_of(e));
        }
    });
    // Deleted objects take their tiles with them.
    delete(doc, &gone);
    for id in now {
        let Some(n) = doc.node(id) else { continue };
        let Some((index, e)) = tiling_of(n) else { continue };
        let sats = satellites_of(e);
        let split = e.visible && e.params.get("split").and_then(Value::as_bool).unwrap_or(false);
        if !split {
            if !sats.is_empty() {
                delete(doc, &sats);
                set_satellites(doc, id, index, &[]);
            }
            continue;
        }
        // Only when the object changed (or its tiles are not made yet). A new object (a copy, a
        // paste) makes tiles of its own rather than take those its list names.
        let fresh = before.node(id).is_none();
        let same = before.node(id).is_some_and(|b| std::ptr::eq(b, n) || b == n);
        if same && !sats.is_empty() {
            continue;
        }
        remake(doc, id, index, fresh);
    }
}

/// Write `sats` as the satellites of effect `index` of object `id`.
fn set_satellites(doc: &mut Document, id: NodeId, index: usize, sats: &[NodeId]) {
    if let Some(e) = doc.node_mut(id).and_then(|n| n.appearance.effects.get_mut(index)) {
        if !e.params.is_object() {
            e.params = effects::merged_params(EFFECT, &Value::Null);
        }
        if let Value::Object(m) = &mut e.params {
            m.insert("satellites".into(), json!(sats.iter().map(|s| s.0).collect::<Vec<_>>()));
        }
    }
}

/// Make object `id`'s tiles (but the first) again as satellites, reusing the ones it has (none
/// when `fresh`).
fn remake(doc: &mut Document, id: NodeId, index: usize, fresh: bool) {
    let Some(n) = doc.node(id) else { return };
    let Some(e) = n.appearance.effects.get(index) else { return };
    let params = effects::merged_params(EFFECT, &e.params);
    let link = params.get("linkStyles").and_then(Value::as_bool).unwrap_or(false);
    let old = if fresh { vec![] } else { satellites_of(e) };
    let tiles: Vec<Affine> = effects::effect_bounds(n)
        .and_then(|b| effects::tiling::layout(b, &params))
        .map(|l| l.tiles.into_iter().skip(1).collect())
        .unwrap_or_default();
    // The tile every satellite is made from: the object without its Tiling (and with its own
    // place, kept by the satellites' transforms).
    let mut base = n.clone();
    base.appearance.effects.retain(|e| e.id != EFFECT);
    let Some((parent, at, _)) = doc.position(id) else { return };
    let mut made = Vec::with_capacity(tiles.len());
    for (k, m) in tiles.iter().enumerate() {
        let mut tile = doc.reid(&base);
        tile.transform(*m, false);
        match old.get(k).copied().filter(|s| doc.node(*s).is_some()) {
            Some(sid) => {
                tile.id = sid;
                if let Some(slot) = doc.node_mut(sid) {
                    if !link {
                        keep_style(slot, &mut tile);
                    }
                    tile.visible = slot.visible;
                    tile.locked = slot.locked;
                    *slot = tile;
                }
                made.push(sid);
            }
            None => {
                let sid = tile.id;
                // Above the object, in tile order.
                let above = at + 1 + made.len();
                if doc.insert(parent, above, tile).is_ok() {
                    made.push(sid);
                }
            }
        }
    }
    // Fewer tiles than before: the extra ones go.
    let extra: Vec<NodeId> = old.iter().skip(made.len()).copied().collect();
    delete(doc, &extra);
    set_satellites(doc, id, index, &made);
}

/// Give `tile` the colours and style `own` has (Link Styles off): its appearance, opacity and
/// blending, and its members' alike while the two have the same members.
fn keep_style(own: &Node, tile: &mut Node) {
    tile.appearance = own.appearance.clone();
    tile.opacity = own.opacity;
    tile.blend = own.blend;
    if let (Some(a), Some(b)) = (own.children(), tile.children_mut())
        && a.len() == b.len()
    {
        for (src, dst) in a.iter().zip(b.iter_mut()) {
            keep_style(src, Arc::make_mut(dst));
        }
    }
}
