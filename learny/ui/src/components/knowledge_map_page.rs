use dioxus::prelude::*;
use dioxus::html::input_data::MouseButton;
use dioxus::document::Stylesheet;
use std::collections::{HashMap, HashSet};
use domain_flashcard::{Card, Deck};
use domain_tagging::Tag;
use crate::app::{Route, TabsCtx};
use crate::api::{
    get_all_tags, get_decks, get_tag_deck_pairs, get_cards_by_tags, get_card_tag_positions, get_tag_edges,
    create_tag, rename_tag, delete_tag, add_tag_edge, remove_tag_edge,
    assign_tag_to_deck, unassign_tag_from_deck, detach_tag_from_cards,
    set_tag_position, clear_tag_position, reset_tag_map,
};

static CSS: Asset = asset!("/assets/knowledge_map.css");

/// In-flight drag of a single node.
#[derive(Clone, Copy)]
struct Drag {
    id: i64,
    start_cx: f64,
    start_cy: f64,
    orig_x: f64,
    orig_y: f64,
    moved: bool,
    /// Whether Shift/Ctrl/Cmd was held — an additive (multi-select) click.
    additive: bool,
    /// Whether Alt/Option was held — a connect click (link parent → child).
    alt: bool,
}

/// In-flight right-button pan of the whole canvas. Records where the drag began
/// and the scroll offset at that moment.
#[derive(Clone, Copy)]
struct Pan {
    start_cx: f64,
    start_cy: f64,
    start_sl: f64,
    start_st: f64,
    moved: bool,
}

/// Cards that have ALL of the given tags (the intersection). With one tag this
/// is just that tag's cards, in that tag's progression order (the backend
/// orders by card_tag.position). With several tags the per-tag order is
/// ambiguous, so the intersection falls back to alphabetical.
async fn cards_intersection(ids: Vec<i64>) -> Vec<Card> {
    if ids.is_empty() {
        return Vec::new();
    }
    let mut lists: Vec<Vec<Card>> = Vec::new();
    for id in &ids {
        lists.push(get_cards_by_tags(vec![*id]).await);
    }
    let sets: Vec<HashSet<i64>> = lists.iter()
        .map(|cs| cs.iter().map(|c| c.id).collect())
        .collect();
    let mut inter = sets[0].clone();
    for s in &sets[1..] {
        inter.retain(|id| s.contains(id));
    }
    if ids.len() == 1 {
        return lists.remove(0).into_iter().filter(|c| inter.contains(&c.id)).collect();
    }
    let card_map: HashMap<i64, Card> = lists.into_iter().flatten().map(|c| (c.id, c)).collect();
    let mut out: Vec<Card> = inter.iter().filter_map(|id| card_map.get(id).cloned()).collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// One row in the selected-tag(s) card list: an order-number gutter followed
/// by the card's deck and name. `nums` carries one (hue, position, tag name)
/// entry per selected tag; `tinted` colours them per tag, which only earns its
/// keep when more than one tag is selected.
#[component]
fn KmCardRow(
    card_id: i64,
    name: String,
    deck: String,
    nums: Vec<(i32, i64, String)>,
    tinted: bool,
) -> Element {
    let tabs_ctx = use_context::<TabsCtx>();
    rsx! {
        div {
            class: "km-card-row",
            onclick: move |_| {
                tabs_ctx.open(Route::CardView { id: card_id });
            },
            div { class: "km-card-nums",
                for (hue , pos , tname) in nums.iter().cloned() {
                    {
                        // The raw stored position, matching the editor's `#`
                        // field. 0 is a real slot — progressions start there.
                        let title = format!("#{pos} in \"{tname}\"");
                        let style = if tinted {
                            format!("color: hsl({hue},60%,32%); background: hsl({hue},60%,92%);")
                        } else {
                            String::new()
                        };
                        rsx! {
                            span {
                                key: "{tname}",
                                class: "km-card-pos",
                                style: "{style}",
                                title: "{title}",
                                "{pos}"
                            }
                        }
                    }
                }
            }
            div { class: "km-card-main",
                span { class: "km-card-deck", "{deck}" }
                span { class: "km-card-name", "{name}" }
            }
        }
    }
}

/// The scrollable canvas container, by id, for reading/writing scroll offsets.
fn canvas_el() -> Option<web_sys::Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("km-canvas-wrap"))
}

/// Approximate node width from its label length, clamped to a sane range.
fn node_width(name: &str) -> f64 {
    (name.chars().count() as f64 * 7.0 + 16.0).clamp(48.0, 230.0)
}

const NODE_H: f64 = 28.0;

/// Distinct hues cycled per deck for node colouring.
const DECK_HUES: [i32; 8] = [210, 145, 35, 350, 275, 110, 320, 175];

// Tidy layout spacing.
const TIDY_VY: f64 = 92.0;    // vertical gap between depth layers
const TIDY_GAP: f64 = 30.0;   // min horizontal gap between node boxes
const TIDY_LEFT: f64 = 120.0;
const TIDY_TOP: f64 = 90.0;

/// Longest-path depth of a node (0 = a root with no eligible parents).
/// Memoised; the graph is acyclic so recursion terminates.
fn node_depth(
    id: i64,
    parents: &HashMap<i64, Vec<i64>>,
    memo: &mut HashMap<i64, usize>,
) -> usize {
    if let Some(d) = memo.get(&id) {
        return *d;
    }
    // Guard against the (already-prevented) cyclic case so we never recurse
    // forever: mark depth 0 before descending.
    memo.insert(id, 0);
    let d = match parents.get(&id) {
        Some(ps) if !ps.is_empty() => {
            1 + ps.iter().map(|p| node_depth(*p, parents, memo)).max().unwrap_or(0)
        }
        _ => 0,
    };
    memo.insert(id, d);
    d
}

/// Place one node and its subtree (Reingold–Tilford style): leaves consume
/// successive horizontal space sized to their width; a parent is centred over
/// its first and last child. `y` comes from the node's DAG depth so layers line
/// up. Returns the node's x.
fn place_tree(
    id: i64,
    children: &HashMap<i64, Vec<i64>>,
    depth: &HashMap<i64, usize>,
    names: &HashMap<i64, String>,
    pos: &mut HashMap<i64, (f64, f64)>,
    cursor: &mut f64,
) -> f64 {
    let y = TIDY_TOP + *depth.get(&id).unwrap_or(&0) as f64 * TIDY_VY;
    let w = node_width(names.get(&id).map(|s| s.as_str()).unwrap_or(""));
    let x = match children.get(&id) {
        Some(kids) if !kids.is_empty() => {
            let xs: Vec<f64> = kids.iter()
                .map(|k| place_tree(*k, children, depth, names, pos, cursor))
                .collect();
            (xs[0] + xs[xs.len() - 1]) / 2.0
        }
        _ => {
            let x = *cursor + w / 2.0;
            *cursor += w + TIDY_GAP;
            x
        }
    };
    pos.insert(id, (x, y));
    x
}

/// Lay out the eligible nodes as a tidy tree. The DAG is reduced to a spanning
/// tree (each node hangs under its shallowest parent); extra parent links are
/// drawn as cross-edges. After placement, a per-layer pass nudges any remaining
/// overlaps apart so wide boxes don't collide.
fn tidy_layout(
    eligible: &HashSet<i64>,
    edges: &[(i64, i64)],
    names: &HashMap<i64, String>,
) -> HashMap<i64, (f64, f64)> {
    let name_of = |id: &i64| names.get(id).cloned().unwrap_or_default().to_lowercase();

    // parents restricted to eligible nodes.
    let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
    for (p, c) in edges {
        if eligible.contains(p) && eligible.contains(c) {
            parents.entry(*c).or_default().push(*p);
        }
    }

    let mut depth: HashMap<i64, usize> = HashMap::new();
    for &id in eligible {
        node_depth(id, &parents, &mut depth);
    }

    // Spanning tree: each node's layout parent is its shallowest parent.
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut has_parent: HashSet<i64> = HashSet::new();
    for &id in eligible {
        if let Some(ps) = parents.get(&id) {
            if let Some(lp) = ps.iter().copied()
                .min_by_key(|p| (*depth.get(p).unwrap_or(&0), *p))
            {
                children.entry(lp).or_default().push(id);
                has_parent.insert(id);
            }
        }
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| name_of(a).cmp(&name_of(b)));
    }

    let mut roots: Vec<i64> = eligible.iter().copied()
        .filter(|id| !has_parent.contains(id))
        .collect();
    roots.sort_by(|a, b| name_of(a).cmp(&name_of(b)));

    // Tree depth: each node sits exactly one level below its layout parent. Using
    // this for Y (rather than the DAG longest-path depth) keeps X and Y on the
    // same tree, so the layout reads as a clean tree instead of drifting
    // diagonally. Cross-edges to other parents are just drawn as curves.
    let mut tree_depth: HashMap<i64, usize> = HashMap::new();
    let mut stack: Vec<(i64, usize)> = roots.iter().map(|&r| (r, 0)).collect();
    while let Some((id, d)) = stack.pop() {
        tree_depth.insert(id, d);
        if let Some(kids) = children.get(&id) {
            for &k in kids {
                stack.push((k, d + 1));
            }
        }
    }

    let mut pos = HashMap::new();
    let mut cursor = TIDY_LEFT;
    for r in roots {
        place_tree(r, &children, &tree_depth, &names, &mut pos, &mut cursor);
        cursor += TIDY_GAP * 2.0; // gap between separate trees
    }

    // Resolve any remaining horizontal overlaps within each depth layer by
    // pushing later nodes to the right until they clear the previous one.
    let max_depth = tree_depth.values().copied().max().unwrap_or(0);
    for d in 0..=max_depth {
        let mut layer: Vec<i64> = pos.keys().copied()
            .filter(|id| *tree_depth.get(id).unwrap_or(&0) == d)
            .collect();
        layer.sort_by(|a, b| {
            pos[a].0.partial_cmp(&pos[b].0).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut min_right = f64::NEG_INFINITY;
        for id in layer {
            let half = node_width(names.get(&id).map(|s| s.as_str()).unwrap_or("")) / 2.0;
            let (mut x, y) = pos[&id];
            if x - half < min_right + TIDY_GAP {
                x = min_right + TIDY_GAP + half;
                pos.insert(id, (x, y));
            }
            min_right = x + half;
        }
    }
    pos
}

/// Scroll the canvas so the given content-space point sits at the viewport centre.
fn center_canvas_on(x: f64, y: f64, zoom: f64) {
    if let Some(el) = canvas_el() {
        let cw = el.client_width() as f64;
        let ch = el.client_height() as f64;
        el.set_scroll_left((x * zoom - cw / 2.0).max(0.0) as i32);
        el.set_scroll_top((y * zoom - ch / 2.0).max(0.0) as i32);
    }
}

/// True if linking `parent` → `child` would create a cycle: i.e. `parent` is
/// already reachable as a descendant of `child` through existing edges.
fn creates_cycle(parent: i64, child: i64, edges: &[(i64, i64)]) -> bool {
    if parent == child {
        return true;
    }
    let mut stack = vec![child];
    let mut seen = HashSet::new();
    while let Some(node) = stack.pop() {
        if node == parent {
            return true;
        }
        for (p, c) in edges {
            if *p == node && seen.insert(*c) {
                stack.push(*c);
            }
        }
    }
    false
}

#[component]
pub fn KnowledgeMapPage() -> Element {
    let tabs_ctx = use_context::<TabsCtx>();

    let mut tags: Signal<Vec<Tag>> = use_signal(Vec::new);
    let mut positions: Signal<HashMap<i64, (f64, f64)>> = use_signal(HashMap::new);
    // Knowledge-map links as (parent_id, child_id). A tag may have many parents.
    let mut edges: Signal<Vec<(i64, i64)>> = use_signal(Vec::new);
    let mut deck_names: Signal<HashMap<i64, String>> = use_signal(HashMap::new);
    let mut loading = use_signal(|| true);

    // Deck filter: which decks each tag belongs to, the deck chip list, and the
    // currently selected decks (empty = show all).
    let mut tag_decks: Signal<HashMap<i64, HashSet<i64>>> = use_signal(HashMap::new);
    let mut decks_list: Signal<Vec<(i64, String)>> = use_signal(Vec::new);
    let mut selected_decks: Signal<HashSet<i64>> = use_signal(HashSet::new);
    let mut sidebar_query = use_signal(String::new);
    let mut new_tag = use_signal(String::new);
    // Chosen destination deck for a new card created from the multi-select panel.
    let mut new_card_deck: Signal<Option<i64>> = use_signal(|| None);
    // Sidebar lists unplaced tags by default; toggle to browse/search placed ones.
    let mut sidebar_placed = use_signal(|| false);
    // When true the sidebar lists tags from every deck (ignoring the deck filter)
    // so existing tags can be reused when building a new deck's map.
    let mut sidebar_show_all = use_signal(|| false);
    // Tag id pending a delete confirmation.
    let mut confirm_delete: Signal<Option<i64>> = use_signal(|| None);
    // Tag id pending a "detach all cards" confirmation.
    let mut confirm_detach: Signal<Option<i64>> = use_signal(|| None);
    // Tag id being renamed (drives the rename dialog), and its working text.
    let mut renaming: Signal<Option<i64>> = use_signal(|| None);
    let mut rename_text = use_signal(String::new);

    let mut zoom = use_signal(|| 1.0_f64);
    // Pending zoom-to-cursor anchor: (content_x, content_y, cursor_x, cursor_y).
    // Applied after the zoom re-render so scroll isn't clamped to the old size.
    let mut zoom_anchor: Signal<Option<(f64, f64, f64, f64)>> = use_signal(|| None);
    // Pending "centre the viewport here" point in content coordinates (set by
    // Tidy). Applied by an effect after the re-render, so the scroll isn't
    // clamped against the canvas's pre-tidy size.
    let mut pending_center: Signal<Option<(f64, f64)>> = use_signal(|| None);
    let mut confirm_clear = use_signal(|| false);
    // Confirmation for "delete all orphan tags" (no deck, no cards).
    let mut confirm_orphans = use_signal(|| false);
    let mut connect_source: Signal<Option<i64>> = use_signal(|| None);
    let mut drag: Signal<Option<Drag>> = use_signal(|| None);
    let mut pan: Signal<Option<Pan>> = use_signal(|| None);

    // Selected tags (multi-select). The cards panel shows the intersection.
    let mut selected: Signal<HashSet<i64>> = use_signal(HashSet::new);
    // Collapsed nodes — their subtrees are hidden on the canvas (view-only).
    let mut collapsed: Signal<HashSet<i64>> = use_signal(HashSet::new);
    let mut cards: Signal<Vec<Card>> = use_signal(Vec::new);
    let mut cards_loading = use_signal(|| false);
    // (tag_id, card_id) → position within that tag's progression, for the
    // currently selected tags. Drives the number gutter in the cards panel.
    let mut card_positions: Signal<HashMap<(i64, i64), i64>> = use_signal(HashMap::new);
    // Deck filter for the selected-tag(s) card list only — separate from the
    // tag's deck assignment chips and from the map's top deck filter bar.
    let mut panel_deck_filter: Signal<HashSet<i64>> = use_signal(HashSet::new);

    // Initial load: tags + decks. Lay out any unplaced node on a grid.
    // Loads (or reloads) all map data from the database. Reused by the initial
    // mount and the toolbar refresh button — so edits to cards/tags made
    // elsewhere show up. It doesn't toggle `loading`, so a refresh updates in
    // place without blanking the canvas.
    let reload = move || async move {
        let loaded = get_all_tags().await;
        let es = get_tag_edges().await;
        let decks: Vec<Deck> = get_decks().await;

        // Deck list + names.
        let mut names: HashMap<i64, String> = HashMap::new();
        let mut dlist: Vec<(i64, String)> = Vec::new();
        for d in &decks {
            names.insert(d.id, d.name.clone());
            dlist.push((d.id, d.name.clone()));
        }
        dlist.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

        // Tag → decks membership (card-derived + explicit assignments) in one call.
        let mut tdecks: HashMap<i64, HashSet<i64>> = HashMap::new();
        for (tag_id, deck_id) in get_tag_deck_pairs().await {
            tdecks.entry(tag_id).or_default().insert(deck_id);
        }

        // Only tags with a saved position appear on the canvas; the rest stay
        // in the sidebar until placed.
        let mut pos = HashMap::new();
        for t in &loaded {
            if let (Some(x), Some(y)) = (t.pos_x, t.pos_y) {
                pos.insert(t.id, (x, y));
            }
        }

        deck_names.set(names);
        tag_decks.set(tdecks);
        decks_list.set(dlist);
        edges.set(es);
        positions.set(pos);
        tags.set(loaded);
        loading.set(false);
    };

    // Initial load.
    use_future(reload);

    // Zoom-to-cursor: after a zoom changes the canvas size, scroll so the point
    // that was under the cursor stays under it.
    let zoom_val = *zoom.read();
    use_effect(use_reactive((&zoom_val,), move |(z,)| {
        let anchor = *zoom_anchor.peek();
        if let Some((cx, cy, mx, my)) = anchor {
            if let Some(el) = canvas_el() {
                el.set_scroll_left((cx * z - mx).max(0.0) as i32);
                el.set_scroll_top((cy * z - my).max(0.0) as i32);
            }
            zoom_anchor.set(None);
        }
    }));

    // Apply a pending centre request once the canvas has re-rendered at its
    // new size. Reading `pending_center` re-runs this after Tidy sets it; the
    // `set(None)` retriggers once more as a no-op.
    use_effect(move || {
        let target = *pending_center.read();
        if let Some((x, y)) = target {
            center_canvas_on(x, y, *zoom.peek());
            pending_center.set(None);
        }
    });

    // Reload the cards panel for the current selection (intersection of tags).
    let mut load_sel_cards = move || {
        let ids: Vec<i64> = selected.read().iter().copied().collect();
        panel_deck_filter.write().clear();
        if ids.is_empty() {
            cards.set(Vec::new());
            card_positions.set(HashMap::new());
            return;
        }
        cards_loading.set(true);
        spawn(async move {
            let positions = get_card_tag_positions(ids.clone()).await;
            card_positions.set(positions.into_iter().map(|(t, c, p)| ((t, c), p)).collect());
            let result = cards_intersection(ids).await;
            cards.set(result);
            cards_loading.set(false);
        });
    };

    // Activate a node. A plain click selects just this tag; an additive click
    // (Shift/Ctrl) toggles it in the multi-selection so the panel can show the
    // intersection; a connect click (Alt/Option) links parent → child.
    let mut activate = move |id: i64, additive: bool, connect: bool| {
        if connect {
            let src = *connect_source.read();
            match src {
                None => connect_source.set(Some(id)),
                Some(s) if s == id => connect_source.set(None),
                Some(s) => {
                    // Link `s` → `id` (s is a parent of id). A tag may have
                    // several parents; skip only if it would cycle or dupe.
                    let exists = edges.read().iter().any(|(p, c)| *p == s && *c == id);
                    if !exists && !creates_cycle(s, id, &edges.read()) {
                        edges.write().push((s, id));
                        spawn(async move {
                            add_tag_edge(s, id).await;
                        });
                    }
                    connect_source.set(None);
                }
            }
        } else {
            // Abandon any half-finished alt-connect when plainly selecting.
            connect_source.set(None);
            {
                let mut s = selected.write();
                if additive {
                    if s.contains(&id) { s.remove(&id); } else { s.insert(id); }
                } else {
                    s.clear();
                    s.insert(id);
                }
            }
            load_sel_cards();
        }
    };

    // Remove a single parent → child link.
    let mut detach_edge = move |parent: i64, child: i64| {
        edges.write().retain(|(p, c)| !(*p == parent && *c == child));
        spawn(async move {
            remove_tag_edge(parent, child).await;
        });
    };

    // Place an unplaced tag near the centre of the currently visible canvas
    // area (falling back to a default if the DOM isn't reachable), with a
    // small cascade so repeated adds don't stack exactly.
    let mut place_tag = move |id: i64| {
        // If a deck filter is active and this tag belongs to none of the selected
        // decks, placing it would leave it invisible under the filter. Auto-assign
        // it to the filtered deck(s) so it shows up where the map is being built.
        let assign_decks: Vec<i64> = {
            let sel = selected_decks.read();
            let td = tag_decks.read();
            let visible = td.get(&id).map_or(false, |s| !s.is_disjoint(&sel));
            if sel.is_empty() || visible {
                Vec::new()
            } else {
                sel.iter().copied().collect()
            }
        };
        if !assign_decks.is_empty() {
            {
                let mut td = tag_decks.write();
                for d in &assign_decks {
                    td.entry(id).or_default().insert(*d);
                }
            }
            for d in assign_decks {
                spawn(async move { assign_tag_to_deck(id, d).await; });
            }
        }

        let z = *zoom.read();
        let (cx, cy) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("km-canvas-wrap"))
            .map(|el| {
                // Viewport-centre in rendered pixels → content coordinates.
                let sl = el.scroll_left() as f64;
                let st = el.scroll_top() as f64;
                let cw = el.client_width() as f64;
                let ch = el.client_height() as f64;
                ((sl + cw / 2.0) / z, (st + ch / 2.0) / z)
            })
            .unwrap_or((320.0, 220.0));
        let off = (positions.read().len() % 6) as f64 * 26.0 - 65.0;
        let x = (cx + off).max(80.0);
        let y = (cy + off).max(70.0);
        positions.write().insert(id, (x, y));
        spawn(async move { set_tag_position(id, x, y).await; });
    };

    // Select a placed tag and scroll the canvas to centre it (used when picking
    // a tag from the sidebar's "On map" list).
    let mut focus_tag = move |id: i64| {
        {
            let mut s = selected.write();
            s.clear();
            s.insert(id);
        }
        load_sel_cards();
        if let (Some((x, y)), Some(el)) = (positions.read().get(&id).copied(), canvas_el()) {
            let z = *zoom.read();
            let cw = el.client_width() as f64;
            let ch = el.client_height() as f64;
            el.set_scroll_left((x * z - cw / 2.0).max(0.0) as i32);
            el.set_scroll_top((y * z - ch / 2.0).max(0.0) as i32);
        }
    };

    // Send the selected tag(s) back to the sidebar.
    let remove_from_map = move |_| {
        let ids: Vec<i64> = selected.read().iter().copied().collect();
        {
            let mut p = positions.write();
            for id in &ids { p.remove(id); }
        }
        selected.write().clear();
        for id in ids {
            spawn(async move { clear_tag_position(id).await; });
        }
    };

    // Remove every placed tag that has no visible parent and no visible child
    // (i.e. floating, unconnected nodes) back to the sidebar.
    let remove_unbound = move |_| {
        let placed: HashSet<i64> = positions.read().keys().copied().collect();
        // A node is "bound" if any edge connects it to another placed node.
        let mut bound: HashSet<i64> = HashSet::new();
        for (p, c) in edges.read().iter() {
            if placed.contains(p) && placed.contains(c) {
                bound.insert(*p);
                bound.insert(*c);
            }
        }
        let loose: Vec<i64> = placed.iter().copied()
            .filter(|id| !bound.contains(id))
            .collect();

        {
            let mut p = positions.write();
            for id in &loose { p.remove(id); }
        }
        selected.write().retain(|sid| !loose.contains(sid));
        for id in loose {
            spawn(async move { clear_tag_position(id).await; });
        }
    };

    // Auto-arrange placed tags into a tidy tree and persist. With no deck filter
    // this lays out the whole structure (anchored top-left). With a filter, it
    // arranges only that deck's visible tags and centres them in the viewport,
    // leaving other decks' tags untouched.
    //
    // `focus` is the node the viewport should end up centred on — the node whose
    // collapse badge was clicked, for instance. Without one we fall back to the
    // single selected tag, then to the layout roots.
    let mut run_tidy_on = move |focus: Option<i64>| {
        let names: HashMap<i64, String> = tags.read().iter()
            .map(|t| (t.id, t.name.clone())).collect();
        let sel_decks = selected_decks.read().clone();
        let tdecks = tag_decks.read().clone();
        let filtered = !sel_decks.is_empty();
        let placed: HashSet<i64> = positions.read().keys().copied().collect();
        let es = edges.read().clone();

        // Deck-matching placed tags (all placed when unfiltered).
        let deck_placed: HashSet<i64> = placed.iter().copied()
            .filter(|id| {
                !filtered || tdecks.get(id).map_or(false, |d| !d.is_disjoint(&sel_decks))
            })
            .collect();
        // Exclude collapsed subtrees so hidden nodes don't reserve layout space —
        // arrange exactly what's visible on the canvas.
        let collapsed_snap = collapsed.read().clone();
        let mut chmap: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut has_parent_in: HashSet<i64> = HashSet::new();
        for (p, c) in &es {
            if deck_placed.contains(p) && deck_placed.contains(c) {
                chmap.entry(*p).or_default().push(*c);
                has_parent_in.insert(*c);
            }
        }
        let mut eligible: HashSet<i64> = HashSet::new();
        let mut stack: Vec<i64> = deck_placed.iter().copied()
            .filter(|id| !has_parent_in.contains(id))
            .collect();
        while let Some(n) = stack.pop() {
            if !eligible.insert(n) {
                continue;
            }
            if !collapsed_snap.contains(&n) {
                if let Some(kids) = chmap.get(&n) {
                    for &c in kids {
                        if !eligible.contains(&c) {
                            stack.push(c);
                        }
                    }
                }
            }
        }
        let layout = tidy_layout(&eligible, &es, &names);
        {
            let mut p = positions.write();
            for (id, xy) in &layout {
                p.insert(*id, *xy);
            }
        }
        // Centre the view on the node in focus so the thing that was just acted
        // on stays put, falling back to the layout root (the roots' midpoint
        // when several trees each have their own root).
        let focus_pt = focus
            .or_else(|| {
                let sel = selected.peek();
                if sel.len() == 1 { sel.iter().next().copied() } else { None }
            })
            .and_then(|id| layout.get(&id).copied());
        if let Some(pt) = focus_pt {
            pending_center.set(Some(pt));
        } else {
            let root_pts: Vec<(f64, f64)> = eligible.iter()
                .filter(|id| !has_parent_in.contains(id))
                .filter_map(|id| layout.get(id).copied())
                .collect();
            if !root_pts.is_empty() {
                let n = root_pts.len() as f64;
                let cx = root_pts.iter().map(|(x, _)| *x).sum::<f64>() / n;
                let cy = root_pts.iter().map(|(_, y)| *y).sum::<f64>() / n;
                pending_center.set(Some((cx, cy)));
            }
        }
        for (id, (x, y)) in layout.into_iter() {
            spawn(async move { set_tag_position(id, x, y).await; });
        }
    };
    let mut run_tidy = move || run_tidy_on(None);

    // Create a brand-new tag (no cards required). It lands in the sidebar, and
    // is auto-assigned to whatever decks are currently filtered so it shows
    // under those decks (and stays out of others).
    let mut create_new_tag = move || {
        let name = new_tag.read().trim().to_string();
        if name.is_empty() {
            return;
        }
        new_tag.set(String::new());
        let assign_decks: Vec<i64> = selected_decks.read().iter().copied().collect();
        spawn(async move {
            if let Some(t) = create_tag(name).await {
                let tid = t.id;
                if !tags.read().iter().any(|x| x.id == tid) {
                    tags.write().push(t);
                }
                if !assign_decks.is_empty() {
                    {
                        let mut td = tag_decks.write();
                        for d in &assign_decks {
                            td.entry(tid).or_default().insert(*d);
                        }
                    }
                    for d in assign_decks {
                        assign_tag_to_deck(tid, d).await;
                    }
                }
            }
        });
    };

    // Toggle a tag's explicit membership in a deck (used by the panel chips).
    let mut toggle_tag_deck = move |tag_id: i64, deck_id: i64| {
        let assigned = tag_decks.read().get(&tag_id).map_or(false, |d| d.contains(&deck_id));
        if assigned {
            if let Some(d) = tag_decks.write().get_mut(&tag_id) {
                d.remove(&deck_id);
            }
            spawn(async move { unassign_tag_from_deck(tag_id, deck_id).await; });
        } else {
            tag_decks.write().entry(tag_id).or_default().insert(deck_id);
            spawn(async move { assign_tag_to_deck(tag_id, deck_id).await; });
        }
    };

    // Delete a tag everywhere: from the map (positions/edges) and the database
    // (which also unlinks it from any cards).
    let mut delete_tag_fn = move |id: i64| {
        tags.write().retain(|t| t.id != id);
        edges.write().retain(|(p, c)| *p != id && *c != id);
        positions.write().remove(&id);
        selected.write().remove(&id);
        if *connect_source.read() == Some(id) {
            connect_source.set(None);
        }
        spawn(async move { delete_tag(id).await; });
    };

    // Delete every orphan tag — one that belongs to no deck and sits on no card,
    // i.e. has no tag↔deck pair at all (a card always lives in a deck, so
    // "has cards" implies "has a deck").
    let delete_orphan_tags = move |_| {
        let ids: Vec<i64> = {
            let td = tag_decks.read();
            tags.read().iter()
                .filter(|t| td.get(&t.id).map_or(true, |d| d.is_empty()))
                .map(|t| t.id)
                .collect()
        };
        let idset: HashSet<i64> = ids.iter().copied().collect();
        tags.write().retain(|t| !idset.contains(&t.id));
        edges.write().retain(|(p, c)| !idset.contains(p) && !idset.contains(c));
        {
            let mut pos = positions.write();
            for id in &idset {
                pos.remove(id);
            }
        }
        selected.write().retain(|id| !idset.contains(id));
        if connect_source.read().map_or(false, |s| idset.contains(&s)) {
            connect_source.set(None);
        }
        confirm_orphans.set(false);
        for id in ids {
            spawn(async move { delete_tag(id).await; });
        }
    };

    // Rename a tag. Updates the name locally (which re-derives every label) and
    // persists; on a blank/duplicate name the backend returns None and the local
    // name is left as-is.
    let rename_tag_fn = move |id: i64, name: String| {
        let name = name.trim().to_string();
        if name.is_empty() {
            return;
        }
        spawn(async move {
            if let Some(t) = rename_tag(id, name).await {
                if let Some(existing) = tags.write().iter_mut().find(|x| x.id == id) {
                    existing.name = t.name;
                }
            }
        });
    };

    // Remove the tag from every card it's on (keeps the tag and its structure),
    // then reload so deck membership/colours update.
    let mut detach_cards_fn = move |id: i64| {
        if selected.read().contains(&id) {
            cards.set(Vec::new());
        }
        spawn(async move {
            detach_tag_from_cards(id).await;
            reload().await;
        });
    };

    // Wipe the whole map: clear every position and parent link.
    let clear_map = move |_| {
        positions.write().clear();
        edges.write().clear();
        selected.write().clear();
        connect_source.set(None);
        confirm_clear.set(false);
        spawn(async move { reset_tag_map().await; });
    };

    // --- Precompute render data ---
    let tags_snapshot = tags.read().clone();
    let pos_snapshot = positions.read().clone();
    let edges_snapshot = edges.read().clone();
    let sel_set = selected.read().clone();
    let src = *connect_source.read();

    // Deck filter: a tag is visible if no decks are selected, or it belongs to a
    // selected deck (membership = card-derived or explicitly assigned). Tags
    // assigned to no deck are shown only in the unfiltered "all decks" view.
    let selected_decks_snap = selected_decks.read().clone();
    let tag_decks_snap = tag_decks.read().clone();

    // Decks actually present among the loaded cards for the current tag
    // selection — the pool the panel's own deck-filter chips draw from.
    let panel_deck_filter_snap = panel_deck_filter.read().clone();
    let mut panel_card_decks: Vec<(i64, String)> = {
        let mut seen = HashSet::new();
        cards.read().iter()
            .filter(|c| seen.insert(c.deck_id))
            .map(|c| (c.deck_id, deck_names.read().get(&c.deck_id).cloned().unwrap_or_default()))
            .collect()
    };
    panel_card_decks.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));

    // The selected tags as (id, name), sorted by name. Their order here is
    // what the panel shows and what tints the per-tag order numbers, so both
    // the chip row and the number gutter agree.
    let mut sel_tags: Vec<(i64, String)> = sel_set.iter()
        .map(|id| (*id, tags_snapshot.iter().find(|t| t.id == *id).map(|t| t.name.clone()).unwrap_or_default()))
        .collect();
    sel_tags.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    // One hue per selected tag, from the same palette the deck nodes use.
    let sel_tag_hue: HashMap<i64, i32> = sel_tags.iter().enumerate()
        .map(|(i, (id, _))| (*id, DECK_HUES[i % DECK_HUES.len()]))
        .collect();
    // With several tags selected the intersection has no inherent order, so
    // the first tag in the list acts as the sort key (its progression),
    // cards missing a position falling to the end in name order.
    let primary_tag = sel_tags.first().map(|(id, _)| *id);
    let positions_snap = card_positions.read().clone();

    // Cards shown in the selected-tag(s) side panel, narrowed by that
    // dedicated filter (empty filter = show every deck).
    let mut panel_cards: Vec<Card> = cards.read().iter()
        .filter(|c| panel_deck_filter_snap.is_empty() || panel_deck_filter_snap.contains(&c.deck_id))
        .cloned()
        .collect();
    if sel_tags.len() > 1 {
        if let Some(pt) = primary_tag {
            panel_cards.sort_by(|a, b| {
                let key = |c: &Card| positions_snap.get(&(pt, c.id)).copied().unwrap_or(i64::MAX);
                key(a).cmp(&key(b)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            });
        }
    }

    // Orphan tags: no deck and no cards (no tag↔deck pair). Drives the
    // toolbar cleanup button and its confirmation dialog.
    let orphan_count = tags_snapshot.iter()
        .filter(|t| tag_decks_snap.get(&t.id).map_or(true, |d| d.is_empty()))
        .count();

    // Per-deck colours (stroke, fill) and deck ordering, for tinting nodes.
    let decks_snapshot = decks_list.read().clone();
    let deck_index: HashMap<i64, usize> = decks_snapshot.iter().enumerate()
        .map(|(i, (id, _))| (*id, i))
        .collect();
    let deck_color: HashMap<i64, (String, String)> = decks_snapshot.iter().enumerate()
        .map(|(i, (id, _))| {
            let h = DECK_HUES[i % DECK_HUES.len()];
            (*id, (format!("hsl({h},55%,45%)"), format!("hsl({h},60%,93%)")))
        })
        .collect();

    let base_visible: HashSet<i64> = tags_snapshot.iter()
        .filter(|t| {
            selected_decks_snap.is_empty()
                || tag_decks_snap.get(&t.id)
                    .map_or(false, |decks| !decks.is_disjoint(&selected_decks_snap))
        })
        .map(|t| t.id)
        .collect();

    let placed_ids: HashSet<i64> = pos_snapshot.keys().copied().collect();
    // child → parents, from the edge list.
    let mut parents_map: HashMap<i64, Vec<i64>> = HashMap::new();
    for (p, c) in &edges_snapshot {
        parents_map.entry(*c).or_default().push(*p);
    }

    // Strict deck filtering: a tag shows only if it belongs to a selected deck.
    // `context_ids` stays empty (kept so the faded-context styling still compiles
    // but never applies).
    let context_ids: HashSet<i64> = HashSet::new();

    // Collapse: hide the subtree of any collapsed node. Canvas nodes are the
    // deck-visible placed tags; a node shows if it's reachable from a root
    // without passing *through* a collapsed node (so a child kept alive by
    // another, non-collapsed parent still shows — correct for a DAG).
    let collapsed_snap = collapsed.read().clone();
    let canvas_nodes: HashSet<i64> = base_visible.iter().copied()
        .filter(|id| placed_ids.contains(id))
        .collect();
    let mut children_map: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut has_parent_in: HashSet<i64> = HashSet::new();
    for (p, c) in &edges_snapshot {
        if canvas_nodes.contains(p) && canvas_nodes.contains(c) {
            children_map.entry(*p).or_default().push(*c);
            has_parent_in.insert(*c);
        }
    }
    let has_children_set: HashSet<i64> = children_map.keys().copied().collect();
    let mut visible_ids: HashSet<i64> = HashSet::new();
    let mut reach_stack: Vec<i64> = canvas_nodes.iter().copied()
        .filter(|id| !has_parent_in.contains(id))
        .collect();
    while let Some(n) = reach_stack.pop() {
        if !visible_ids.insert(n) {
            continue;
        }
        if !collapsed_snap.contains(&n) {
            if let Some(kids) = children_map.get(&n) {
                for &c in kids {
                    if !visible_ids.contains(&c) {
                        reach_stack.push(c);
                    }
                }
            }
        }
    }

    // The sidebar shows either unplaced or on-map tags (toggle), filtered by the
    // deck filter and the search box.
    let placed_mode = *sidebar_placed.read();
    let show_all_tags = *sidebar_show_all.read();
    // The pool the sidebar draws from: the deck-filtered set normally, or every
    // tag when "All tags" is on (to reuse existing tags across decks).
    let sidebar_pool: HashSet<i64> = if show_all_tags {
        tags_snapshot.iter().map(|t| t.id).collect()
    } else {
        base_visible.clone()
    };
    let sidebar_filter = sidebar_query.read().trim().to_lowercase();
    let mut sidebar_tags: Vec<Tag> = tags_snapshot.iter()
        .filter(|t| sidebar_pool.contains(&t.id))
        .filter(|t| if placed_mode { placed_ids.contains(&t.id) } else { !placed_ids.contains(&t.id) })
        .filter(|t| sidebar_filter.is_empty() || t.name.to_lowercase().contains(&sidebar_filter))
        .cloned()
        .collect();
    sidebar_tags.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let unplaced_total = tags_snapshot.iter()
        .filter(|t| sidebar_pool.contains(&t.id) && !placed_ids.contains(&t.id))
        .count();
    let placed_total = tags_snapshot.iter()
        .filter(|t| sidebar_pool.contains(&t.id) && placed_ids.contains(&t.id))
        .count();
    let mode_total = if placed_mode { placed_total } else { unplaced_total };
    let canvas_count = tags_snapshot.iter()
        .filter(|t| visible_ids.contains(&t.id) && placed_ids.contains(&t.id))
        .count();

    // Neighbours of the selected node(s) (either direction) — used to highlight
    // connections and dim everything else.
    let has_selection = !sel_set.is_empty();
    let mut neighbor_ids: HashSet<i64> = HashSet::new();
    for (p, c) in &edges_snapshot {
        if sel_set.contains(p) { neighbor_ids.insert(*c); }
        if sel_set.contains(c) { neighbor_ids.insert(*p); }
    }

    let name_by_id: HashMap<i64, String> = tags_snapshot.iter()
        .map(|t| (t.id, t.name.clone()))
        .collect();
    let half_h = NODE_H / 2.0;
    // Stop each edge at the node *border* (not centre) so arrowheads stay
    // visible even when nodes are far apart and the boxes are wide, then route
    // it as a smooth cubic bezier eased along the dominant axis. `hl` marks
    // edges touching the selected node.
    let edge_paths: Vec<(String, bool)> = edges_snapshot.iter()
        .filter_map(|(p, c)| {
            if !visible_ids.contains(p) || !visible_ids.contains(c) {
                return None;
            }
            let (cx, cy) = pos_snapshot.get(c).copied()?;
            let (px, py) = pos_snapshot.get(p).copied()?;
            let (dx, dy) = (cx - px, cy - py);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1.0 { return None; }
            let (ux, uy) = (dx / len, dy / len);
            // Distance from a node centre to where the ray exits its box.
            let border = |id: &i64| {
                let hw = name_by_id.get(id).map(|n| node_width(n)).unwrap_or(80.0) / 2.0;
                let tx = if ux.abs() < 1e-3 { f64::INFINITY } else { hw / ux.abs() };
                let ty = if uy.abs() < 1e-3 { f64::INFINITY } else { half_h / uy.abs() };
                tx.min(ty)
            };
            let t_p = border(p);
            let t_c = border(c) + 7.0; // small gap so the arrowhead sits outside
            if t_p + t_c >= len { return None; } // boxes overlap — skip
            let (x1, y1) = (px + ux * t_p, py + uy * t_p);
            let (x2, y2) = (cx - ux * t_c, cy - uy * t_c);

            // Ease control points along the dominant axis for a clean S-curve
            // that leaves the parent and enters the child head-on.
            let (sx, sy) = (x2 - x1, y2 - y1);
            let k = 0.5;
            let (c1x, c1y, c2x, c2y) = if sy.abs() >= sx.abs() {
                (x1, y1 + sy * k, x2, y2 - sy * k)
            } else {
                (x1 + sx * k, y1, x2 - sx * k, y2)
            };
            let d = format!(
                "M {x1:.1} {y1:.1} C {c1x:.1} {c1y:.1} {c2x:.1} {c2y:.1} {x2:.1} {y2:.1}"
            );
            let hl = sel_set.contains(p) || sel_set.contains(c);
            Some((d, hl))
        })
        .collect();

    // Canvas size from the furthest node (in content coordinates).
    let (mut canvas_w, mut canvas_h) = (1200.0_f64, 800.0_f64);
    for (x, y) in pos_snapshot.values() {
        canvas_w = canvas_w.max(x + 200.0);
        canvas_h = canvas_h.max(y + 160.0);
    }
    // Zoom scales the rendered SVG; the viewBox keeps content coordinates fixed.
    let z = *zoom.read();
    let svg_w = canvas_w * z;
    let svg_h = canvas_h * z;
    let zoom_pct = (z * 100.0).round() as i32;

    // The single-tag panel shows only when exactly one tag is selected.
    let single_sel = if sel_set.len() == 1 { sel_set.iter().copied().next() } else { None };
    let selected_tag = single_sel.and_then(|id| tags_snapshot.iter().find(|t| t.id == id).cloned());
    let name_of_id = |id: i64| tags_snapshot.iter().find(|t| t.id == id).map(|t| t.name.clone()).unwrap_or_default();
    // Parents of the single selected node, as (id, name).
    let mut selected_parents: Vec<(i64, String)> = single_sel
        .and_then(|id| parents_map.get(&id))
        .map(|ps| ps.iter().map(|p| (*p, name_of_id(*p))).collect())
        .unwrap_or_default();
    selected_parents.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    // Children of the single selected node (edges where it is the parent).
    let mut selected_children: Vec<(i64, String)> = single_sel
        .map(|id| edges_snapshot.iter()
            .filter(|(p, _)| *p == id)
            .map(|(_, c)| (*c, name_of_id(*c)))
            .collect())
        .unwrap_or_default();
    selected_children.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    // The multi-select panel lists the selected tags (id, name), sorted.
    let multi_sel = sel_tags.clone();

    rsx! {
        Stylesheet { href: CSS }
        div { class: "km-page",

            // ---- Toolbar ----
            div { class: "km-toolbar",
                div { class: "km-modes",
                    button {
                        class: "km-mode-btn",
                        title: "Auto-arrange placed tags using the chosen layout",
                        onclick: move |_| run_tidy(),
                        "✦ Tidy"
                    }
                    button {
                        class: "km-mode-btn",
                        title: "Expand every collapsed node and re-arrange",
                        onclick: move |_| {
                            collapsed.write().clear();
                            run_tidy();
                        },
                        "⊞ Expand all"
                    }
                    button {
                        class: "km-mode-btn",
                        title: "Collapse every node that has children and re-arrange",
                        onclick: {
                            let hc = has_children_set.clone();
                            move |_| {
                                *collapsed.write() = hc.clone();
                                run_tidy();
                            }
                        },
                        "⊟ Collapse all"
                    }
                    button {
                        class: "km-mode-btn",
                        title: "Remove all unconnected tags from the map",
                        onclick: remove_unbound,
                        "⊘ Remove unbound"
                    }
                    button {
                        class: "km-mode-btn km-mode-danger",
                        title: "Delete all tags that belong to no deck and no card",
                        onclick: move |_| confirm_orphans.set(true),
                        "🧹 Delete orphans"
                    }
                    button {
                        class: "km-mode-btn",
                        title: "Reload tags, cards and connections from the database",
                        onclick: move |_| { spawn(reload()); },
                        "↻ Refresh"
                    }
                    button {
                        class: "km-mode-btn km-mode-danger",
                        title: "Clear the entire map (positions and connections)",
                        onclick: move |_| confirm_clear.set(true),
                        "🗑 Clear map"
                    }
                }
                if src.is_some() {
                    span { class: "km-hint",
                        "Now click the child tag (click the same tag to cancel)"
                    }
                }
                div { class: "km-zoom",
                    button {
                        class: "km-zoom-btn",
                        title: "Zoom out",
                        onclick: move |_| {
                            let n = (*zoom.read() / 1.2).clamp(0.3, 3.0);
                            zoom.set(n);
                        },
                        "−"
                    }
                    button {
                        class: "km-zoom-level",
                        title: "Reset zoom (zoom with Ctrl + scroll or trackpad pinch)",
                        onclick: move |_| zoom.set(1.0),
                        "{zoom_pct}%"
                    }
                    button {
                        class: "km-zoom-btn",
                        title: "Zoom in",
                        onclick: move |_| {
                            let n = (*zoom.read() * 1.2).clamp(0.3, 3.0);
                            zoom.set(n);
                        },
                        "+"
                    }
                }
            }

            // ---- Deck filter bar ----
            if !decks_list.read().is_empty() {
                div { class: "km-deckbar",
                    for (id , name) in decks_list.read().iter().cloned() {
                        {
                            let is_sel = selected_decks.read().contains(&id);
                            let swatch = deck_color.get(&id).map(|(s, _)| s.clone()).unwrap_or_default();
                            rsx! {
                                button {
                                    class: if is_sel { "km-deck-chip km-deck-chip-active" } else { "km-deck-chip" },
                                    onclick: move |_| {
                                        let mut s = selected_decks.write();
                                        if s.contains(&id) {
                                            s.remove(&id);
                                        } else {
                                            s.insert(id);
                                        }
                                    },
                                    span { class: "km-deck-swatch", style: "background:{swatch}" }
                                    "{name}"
                                }
                            }
                        }
                    }
                    if !selected_decks.read().is_empty() {
                        button {
                            class: "km-deck-clear",
                            onclick: move |_| selected_decks.write().clear(),
                            "Show all"
                        }
                    }
                }
            }

            div { class: "km-body",
                // ---- Sidebar: tags ----
                div { class: "km-sidebar",
                    div { class: "km-sidebar-toggle",
                        button {
                            class: if !placed_mode { "km-seg km-seg-active" } else { "km-seg" },
                            onclick: move |_| sidebar_placed.set(false),
                            "Unplaced"
                        }
                        button {
                            class: if placed_mode { "km-seg km-seg-active" } else { "km-seg" },
                            onclick: move |_| sidebar_placed.set(true),
                            "On map"
                        }
                    }
                    div { class: "km-sidebar-toggle",
                        button {
                            class: if !show_all_tags { "km-seg km-seg-active" } else { "km-seg" },
                            title: "List only tags belonging to the selected deck(s)",
                            onclick: move |_| sidebar_show_all.set(false),
                            "Deck tags"
                        }
                        button {
                            class: if show_all_tags { "km-seg km-seg-active" } else { "km-seg" },
                            title: "List tags from every deck — reuse existing tags when building a new map",
                            onclick: move |_| sidebar_show_all.set(true),
                            "All tags"
                        }
                    }

                    // Create a new tag (always available; new tags start unplaced).
                    if !placed_mode {
                        p { class: "km-sidebar-hint",
                            "Create tags here (no cards needed), then click one to add it to the map."
                        }
                        div { class: "km-sidebar-create",
                            input {
                                class: "km-sidebar-new",
                                placeholder: "New tag name…",
                                value: "{new_tag}",
                                oninput: move |e| new_tag.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter { create_new_tag(); }
                                },
                            }
                            button {
                                class: "km-sidebar-add",
                                title: "Create tag",
                                onclick: move |_| create_new_tag(),
                                "+"
                            }
                        }
                    }

                    if mode_total > 4 {
                        input {
                            class: "km-sidebar-search",
                            placeholder: "Search tags…",
                            value: "{sidebar_query}",
                            oninput: move |e| sidebar_query.set(e.value()),
                        }
                    }

                    if sidebar_tags.is_empty() {
                        p { class: "km-sidebar-empty",
                            if mode_total == 0 {
                                if placed_mode { "No tags on the map yet." } else { "No unplaced tags." }
                            } else {
                                "No matching tags."
                            }
                        }
                    } else {
                        div { class: "km-sidebar-list",
                            for t in sidebar_tags.iter().cloned() {
                                {
                                    let tid = t.id;
                                    rsx! {
                                        div { class: "km-sidebar-row",
                                            button {
                                                class: "km-sidebar-tag",
                                                title: if placed_mode { "Show on map" } else { "Add to map" },
                                                onclick: move |_| {
                                                    if placed_mode { focus_tag(tid) } else { place_tag(tid) }
                                                },
                                                "{t.name}"
                                            }
                                            button {
                                                class: "km-sidebar-del",
                                                title: "Delete tag",
                                                onclick: move |_| confirm_delete.set(Some(tid)),
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---- Canvas ----
                div { class: "km-canvas-wrap", id: "km-canvas-wrap",
                    if *loading.read() {
                        p { class: "km-empty", "Loading tags…" }
                    } else if canvas_count == 0 {
                        p { class: "km-empty",
                            "Nothing placed yet — click tags in the sidebar to add them, then hold Alt/Option and click two tags to connect them."
                        }
                    } else {
                        svg {
                            class: "km-canvas",
                            width: "{svg_w}",
                            height: "{svg_h}",
                            view_box: "0 0 {canvas_w} {canvas_h}",
                            // Zoom only while Ctrl is held (or a Mac trackpad
                            // pinch, which fires a wheel event with ctrlKey set).
                            // A plain wheel is left alone so it scrolls/pans.
                            onwheel: move |e| {
                                let m = e.modifiers();
                                if !(m.ctrl() || m.meta()) {
                                    return;
                                }
                                e.prevent_default();
                                let dy = e.delta().strip_units().y;
                                if dy == 0.0 {
                                    return;
                                }
                                // Scale the zoom step by the gesture magnitude so a
                                // trackpad pinch (many tiny deltas) is smooth, while
                                // clamping each step so a chunky mouse-wheel notch
                                // still behaves.
                                let old = *zoom.read();
                                let factor = (-dy * 0.01).exp().clamp(1.0 / 1.1, 1.1);
                                let next = (old * factor).clamp(0.3, 3.0);
                                if (next - old).abs() < 1e-6 {
                                    return;
                                }
                                // Record the content point under the cursor so the
                                // post-render effect can keep it anchored there.
                                if let Some(el) = canvas_el() {
                                    let rect = el.get_bounding_client_rect();
                                    let c = e.client_coordinates();
                                    let mx = c.x - rect.left();
                                    let my = c.y - rect.top();
                                    let cx = (el.scroll_left() as f64 + mx) / old;
                                    let cy = (el.scroll_top() as f64 + my) / old;
                                    zoom_anchor.set(Some((cx, cy, mx, my)));
                                }
                                zoom.set(next);
                            },
                            // Left-press on empty canvas starts a pan. (Presses on
                            // a node stop propagation, so they drag the node instead.)
                            onpointerdown: move |e| {
                                if e.trigger_button() == Some(MouseButton::Primary) {
                                    if let Some(el) = canvas_el() {
                                        let c = e.client_coordinates();
                                        pan.set(
                                            Some(Pan {
                                                start_cx: c.x,
                                                start_cy: c.y,
                                                start_sl: el.scroll_left() as f64,
                                                start_st: el.scroll_top() as f64,
                                                moved: false,
                                            }),
                                        );
                                    }
                                }
                            },
                            // Pan the canvas, or live-update a dragged node.
                            // Pixel deltas are divided by zoom for nodes.
                            onpointermove: move |e| {
                                let cur_pan = *pan.read();
                                if let Some(mut p) = cur_pan {
                                    let c = e.client_coordinates();
                                    if (c.x - p.start_cx).abs() > 3.0 || (c.y - p.start_cy).abs() > 3.0 {
                                        p.moved = true;
                                        pan.set(Some(p));
                                    }
                                    if let Some(el) = canvas_el() {
                                        el.set_scroll_left((p.start_sl - (c.x - p.start_cx)) as i32);
                                        el.set_scroll_top((p.start_st - (c.y - p.start_cy)) as i32);
                                    }
                                    return;
                                }
                                let current = *drag.read();
                                if let Some(mut d) = current {
                                    let zf = *zoom.read();
                                    let c = e.client_coordinates();
                                    let nx = d.orig_x + (c.x - d.start_cx) / zf;
                                    let ny = d.orig_y + (c.y - d.start_cy) / zf;
                                    if (c.x - d.start_cx).abs() > 3.0 || (c.y - d.start_cy).abs() > 3.0 {
                                        d.moved = true;
                                        drag.set(Some(d));
                                    }
                                    positions.write().insert(d.id, (nx, ny));
                                }
                            },
                            onpointerup: move |_| {
                                let cur_pan = *pan.read();
                                if let Some(p) = cur_pan {
                                    pan.set(None);
                                    // A press with no drag = click on empty space → clear selection.
                                    if !p.moved {
                                        selected.write().clear();
                                        connect_source.set(None);
                                    }
                                    return;
                                }
                                let current = *drag.read();
                                if let Some(d) = current {
                                    drag.set(None);
                                    if d.moved {
                                        if let Some((x, y)) = positions.read().get(&d.id).copied() {
                                            spawn(async move {
                                                set_tag_position(d.id, x, y).await;
                                            });
                                        }
                                    } else {
                                        activate(d.id, d.additive, d.alt);
                                    }
                                }
                            },

                            // Background — pannable surface; a press that doesn't
                            // drag clears the selection (handled in onpointerup).
                            rect {
                                class: "km-bg",
                                x: "0",
                                y: "0",
                                width: "{canvas_w}",
                                height: "{canvas_h}",
                            }

                            // Arrowhead markers (parent → child direction).
                            defs {
                                marker {
                                    id: "km-arrow",
                                    view_box: "0 0 10 10",
                                    ref_x: "8",
                                    ref_y: "5",
                                    marker_width: "8",
                                    marker_height: "8",
                                    orient: "auto-start-reverse",
                                    path {
                                        class: "km-arrow-head",
                                        d: "M 0 0 L 10 5 L 0 10 z",
                                    }
                                }
                                marker {
                                    id: "km-arrow-hl",
                                    view_box: "0 0 10 10",
                                    ref_x: "8",
                                    ref_y: "5",
                                    marker_width: "7",
                                    marker_height: "7",
                                    orient: "auto-start-reverse",
                                    path {
                                        class: "km-arrow-head-hl",
                                        d: "M 0 0 L 10 5 L 0 10 z",
                                    }
                                }
                            }

                            // Edges (curved)
                            for (d , hl) in edge_paths {
                                path {
                                    class: if hl { "km-edge km-edge-hl" } else if has_selection { "km-edge km-edge-dim" } else { "km-edge" },
                                    d: "{d}",
                                    marker_end: if hl { "url(#km-arrow-hl)" } else { "url(#km-arrow)" },
                                }
                            }

                            // Nodes
                            for t in tags_snapshot
                                .iter()
                                .filter(|t| visible_ids.contains(&t.id) && placed_ids.contains(&t.id))
                                .cloned()
                            {
                                {
                                    let (x, y) = pos_snapshot.get(&t.id).copied().unwrap_or((0.0, 0.0));
                                    let w = node_width(&t.name);
                                    let is_sel = sel_set.contains(&t.id);
                                    let is_src = src == Some(t.id);
                                    let cls = if is_src {
                                        "km-node-rect km-node-source"
                                    } else if is_sel {
                                        "km-node-rect km-node-selected"
                                    } else {
                                        "km-node-rect"
                                    };
                                    let tid = t.id;
                                    // Dim nodes unrelated to the current selection.
                                    let dimmed = has_selection && !is_sel && !neighbor_ids.contains(&tid);
                                    let g_cls = if context_ids.contains(&tid) {
                                        "km-node km-node-ctx"
                                    } else if dimmed {
                                        "km-node km-node-dim"
                                    } else {
                                        "km-node"
                                    };
                                    // All decks this tag belongs to, ordered, with colours —
                                    // shown as a segmented bar along the bottom of the node.
                                    let mut bar_decks: Vec<i64> = tag_decks_snap.get(&tid)
                                        .map(|s| s.iter().copied().filter(|d| deck_index.contains_key(d)).collect())
                                        .unwrap_or_default();
                                    bar_decks.sort_by_key(|d| deck_index[d]);
                                    let bar_n = bar_decks.len();
                                    let bar_w = if bar_n > 0 { (w - 12.0) / bar_n as f64 } else { 0.0 };
                                    rsx! {
                                        g {
                                            class: "{g_cls}",
                                            transform: "translate({x},{y})",
                                            onpointerdown: move |e| {
                                                if e.trigger_button() != Some(MouseButton::Primary) {
                                                    return;
                                                }
                                                // Don't let the press bubble to the canvas pan handler.
                                                e.stop_propagation();
                                                let m = e.modifiers();
                                                let c = e.client_coordinates();
                                                let (ox, oy) = positions.read().get(&tid).copied().unwrap_or((0.0, 0.0));
                                                drag.set(
                                                    Some(Drag {
                                                        id: tid,
                                                        start_cx: c.x,
                                                        start_cy: c.y,
                                                        orig_x: ox,
                                                        orig_y: oy,
                                                        moved: false,
                                                        additive: m.ctrl() || m.meta() || m.shift(),
                                                        alt: m.alt(),
                                                    }),
                                                );
                                            },
                                            rect {
                                                class: "{cls}",
                                                x: "{-w / 2.0}",
                                                y: "{-NODE_H / 2.0}",
                                                width: "{w}",
                                                height: "{NODE_H}",
                                                rx: "7",
                                            }
                                            text {
                                                class: "km-node-label",
                                                x: "0",
                                                y: "2",
                                                text_anchor: "middle",
                                                "{t.name}"
                                            }
                                            // Deck-colour bar (one segment per deck).
                                            for (i , d) in bar_decks.iter().enumerate() {
                                                rect {
                                                    class: "km-node-bar",
                                                    x: "{-w / 2.0 + 6.0 + i as f64 * bar_w}",
                                                    y: "{NODE_H / 2.0 - 5.0}",
                                                    width: "{bar_w}",
                                                    height: "3.5",
                                                    style: "fill:{deck_color.get(d).map(|(s, _)| s.clone()).unwrap_or_default()}",
                                                }
                                            }
                                            // Collapse/expand toggle for nodes with children.
                                            if has_children_set.contains(&tid) {
                                                {
                                                    let is_collapsed = collapsed_snap.contains(&tid);
                                                    rsx! {
                                                        circle {
                                                            class: "km-collapse-badge",
                                                            cx: "{w / 2.0 + 1.0}",
                                                            cy: "0",
                                                            r: "7",
                                                            onpointerdown: move |e| e.stop_propagation(),
                                                            onclick: move |e| {
                                                                e.stop_propagation();
                                                                {
                                                                    let mut cs = collapsed.write();
                                                                    if cs.contains(&tid) { cs.remove(&tid); } else { cs.insert(tid); }
                                                                }
                                                                // Re-arrange so the freed/reclaimed space closes
                                                                // up, keeping this node under the viewport centre.
                                                                run_tidy_on(Some(tid));
                                                            },
                                                        }
                                                        text {
                                                            class: "km-collapse-sign",
                                                            x: "{w / 2.0 + 1.0}",
                                                            y: "3",
                                                            text_anchor: "middle",
                                                            if is_collapsed { "+" } else { "−" }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---- Side panel: single selected tag ----
                if let Some(tag) = selected_tag {
                    div { class: "km-panel",
                        div { class: "km-panel-head",
                            span { class: "km-panel-title", "{tag.name}" }
                            button {
                                class: "km-panel-close",
                                title: "Close",
                                onclick: move |_| selected.write().clear(),
                                "✕"
                            }
                        }
                        if !selected_parents.is_empty() {
                            p { class: "km-panel-hint", "Parents" }
                            div { class: "km-parent-list",
                                for (pid , pname) in selected_parents.iter().cloned() {
                                    {
                                        let child = tag.id;
                                        rsx! {
                                            div { class: "km-parent-row",
                                                button {
                                                    class: "km-parent-name km-rel-link",
                                                    title: "Go to \"{pname}\" on the map",
                                                    onclick: move |_| focus_tag(pid),
                                                    "{pname}"
                                                }
                                                button {
                                                    class: "km-parent-detach",
                                                    title: "Remove this parent link",
                                                    onclick: move |_| detach_edge(pid, child),
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !selected_children.is_empty() {
                            p { class: "km-panel-hint", "Children" }
                            div { class: "km-parent-list",
                                for (cid , cname) in selected_children.iter().cloned() {
                                    {
                                        let parent = tag.id;
                                        rsx! {
                                            div { class: "km-parent-row",
                                                button {
                                                    class: "km-parent-name km-rel-link",
                                                    title: "Go to \"{cname}\" on the map",
                                                    onclick: move |_| focus_tag(cid),
                                                    "{cname}"
                                                }
                                                button {
                                                    class: "km-parent-detach",
                                                    title: "Remove this child link",
                                                    onclick: move |_| detach_edge(parent, cid),
                                                    "✕"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !decks_list.read().is_empty() {
                            p { class: "km-panel-hint", "Decks" }
                            div { class: "km-panel-decks",
                                for (did , dname) in decks_list.read().iter().cloned() {
                                    {
                                        let tagid = tag.id;
                                        let active = tag_decks.read().get(&tagid)
                                            .map_or(false, |d| d.contains(&did));
                                        rsx! {
                                            button {
                                                class: if active { "km-deck-chip km-deck-chip-active" } else { "km-deck-chip" },
                                                title: "Assign / unassign this tag to the deck",
                                                onclick: move |_| toggle_tag_deck(tagid, did),
                                                "{dname}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        div { class: "km-panel-actions",
                            {
                                let tid = tag.id;
                                let tname = tag.name.clone();
                                rsx! {
                                    button {
                                        class: "km-detach-btn",
                                        title: "Rename this tag",
                                        onclick: move |_| {
                                            rename_text.set(tname.clone());
                                            renaming.set(Some(tid));
                                        },
                                        "Rename"
                                    }
                                }
                            }
                            button {
                                class: "km-detach-btn",
                                onclick: remove_from_map,
                                "Remove from map"
                            }
                            {
                                let tid = tag.id;
                                rsx! {
                                    button {
                                        class: "km-detach-btn",
                                        title: "Remove this tag from every card (keeps the tag)",
                                        onclick: move |_| confirm_detach.set(Some(tid)),
                                        "Detach cards"
                                    }
                                }
                            }
                            {
                                let tid = tag.id;
                                rsx! {
                                    button {
                                        class: "km-detach-btn km-detach-danger",
                                        title: "Delete this tag from every card and the map",
                                        onclick: move |_| confirm_delete.set(Some(tid)),
                                        "Delete tag"
                                    }
                                }
                            }
                        }
                        // Cards header with a quick "new card" button. The card is
                        // created in a deck this tag belongs to (or the first deck).
                        {
                            let target_deck = {
                                let tdecks = tag_decks_snap.get(&tag.id);
                                selected_decks_snap.iter().copied()
                                    .find(|d| tdecks.map_or(false, |s| s.contains(d)))
                                    .or_else(|| tdecks.and_then(|s| {
                                        s.iter().copied().min_by_key(|d| deck_index.get(d).copied().unwrap_or(usize::MAX))
                                    }))
                                    .or_else(|| decks_snapshot.first().map(|(id, _)| *id))
                            };
                            rsx! {
                                div { class: "km-panel-cardhead",
                                    span { class: "km-panel-hint", "Cards" }
                                    if let Some(d) = target_deck {
                                        {
                                            let tag_name = tag.name.clone();
                                            rsx! {
                                                button {
                                                    class: "km-card-add",
                                                    title: "New card in this deck (pre-tagged)",
                                                    onclick: move |_| {
                                                        *crate::app::PENDING_NEW_CARD_TAGS.write() = vec![tag_name.clone()];
                                                        tabs_ctx.open(Route::CardEditorNew { deck_id: d });
                                                    },
                                                    "+"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if panel_card_decks.len() > 1 {
                            div { class: "km-panel-cardfilter",
                                for (did , dname) in panel_card_decks.iter().cloned() {
                                    {
                                        let is_sel = panel_deck_filter_snap.contains(&did);
                                        rsx! {
                                            button {
                                                class: if is_sel { "km-deck-chip km-deck-chip-active" } else { "km-deck-chip" },
                                                title: "Show only cards in this deck",
                                                onclick: move |_| {
                                                    let mut f = panel_deck_filter.write();
                                                    if f.contains(&did) { f.remove(&did); } else { f.insert(did); }
                                                },
                                                "{dname}"
                                            }
                                        }
                                    }
                                }
                                if !panel_deck_filter_snap.is_empty() {
                                    button {
                                        class: "km-deck-clear",
                                        onclick: move |_| panel_deck_filter.write().clear(),
                                        "Show all"
                                    }
                                }
                            }
                        }
                        if *cards_loading.read() {
                            p { class: "km-panel-hint", "Loading cards…" }
                        } else if cards.read().is_empty() {
                            p { class: "km-panel-hint", "No cards with this tag." }
                        } else if panel_cards.is_empty() {
                            p { class: "km-panel-hint", "No cards in the selected deck(s)." }
                        } else {
                            p { class: "km-panel-hint",
                                if panel_deck_filter_snap.is_empty() {
                                    "{panel_cards.len()} card(s) across all decks"
                                } else {
                                    "{panel_cards.len()} card(s) in the filtered deck(s)"
                                }
                            }
                            div { class: "km-card-list",
                                for card in panel_cards.iter().cloned() {
                                    {
                                        let deck_name = deck_names
                                            .read()
                                            .get(&card.deck_id)
                                            .cloned()
                                            .unwrap_or_default();
                                        let pos = positions_snap
                                            .get(&(tag.id, card.id))
                                            .copied()
                                            .unwrap_or(0);
                                        rsx! {
                                            KmCardRow {
                                                key: "{card.id}",
                                                card_id: card.id,
                                                name: card.name.clone(),
                                                deck: deck_name,
                                                nums: vec![(0, pos, tag.name.clone())],
                                                tinted: false,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ---- Side panel: multiple selected tags (intersection) ----
                if multi_sel.len() > 1 {
                    div { class: "km-panel",
                        div { class: "km-panel-head",
                            span { class: "km-panel-title", "{multi_sel.len()} tags selected" }
                            button {
                                class: "km-panel-close",
                                title: "Clear selection",
                                onclick: move |_| selected.write().clear(),
                                "✕"
                            }
                        }
                        p { class: "km-panel-hint", "Shift/Ctrl-click nodes to add. Showing cards with all of:" }
                        div { class: "km-panel-decks",
                            for (tid , tname) in multi_sel.iter().cloned() {
                                {
                                    // Same hue as this tag's numbers in the card
                                    // list below, so the columns are readable.
                                    let hue = sel_tag_hue.get(&tid).copied().unwrap_or(0);
                                    rsx! {
                                        button {
                                            key: "{tid}",
                                            class: "km-deck-chip km-deck-chip-active km-tag-chip-tinted",
                                            style: "background: hsl({hue},60%,92%); color: hsl({hue},60%,28%); border-color: hsl({hue},45%,72%);",
                                            title: "Remove from selection",
                                            onclick: move |_| { selected.write().remove(&tid); load_sel_cards(); },
                                            "{tname} ✕"
                                        }
                                    }
                                }
                            }
                        }
                        if let Some((_, primary_name)) = multi_sel.first().cloned() {
                            p { class: "km-panel-hint",
                                "Numbered per tag, in chip order — ordered by \u{201c}{primary_name}\u{201d}."
                            }
                        }
                        // New card pre-tagged with all selected tags, in a chosen deck.
                        {
                            let names: Vec<String> = multi_sel.iter().map(|(_, n)| n.clone()).collect();
                            // Default deck: an active filter deck, else one a selected tag is
                            // in, else the first deck. The user can override via the chips.
                            let auto_deck = selected_decks_snap.iter().copied()
                                .min_by_key(|d| deck_index.get(d).copied().unwrap_or(usize::MAX))
                                .or_else(|| {
                                    multi_sel.iter()
                                        .filter_map(|(tid, _)| tag_decks_snap.get(tid))
                                        .flat_map(|s| s.iter().copied())
                                        .min_by_key(|d| deck_index.get(d).copied().unwrap_or(usize::MAX))
                                })
                                .or_else(|| decks_snapshot.first().map(|(id, _)| *id));
                            let target_deck = (*new_card_deck.read()).or(auto_deck);
                            rsx! {
                                p { class: "km-panel-hint", "New card deck" }
                                div { class: "km-panel-decks",
                                    for (did , dname) in decks_snapshot.iter().cloned() {
                                        button {
                                            class: if target_deck == Some(did) { "km-deck-chip km-deck-chip-active" } else { "km-deck-chip" },
                                            onclick: move |_| new_card_deck.set(Some(did)),
                                            "{dname}"
                                        }
                                    }
                                }
                                div { class: "km-panel-cardhead",
                                    span { class: "km-panel-hint", "Cards" }
                                    if let Some(d) = target_deck {
                                        button {
                                            class: "km-card-add",
                                            title: "New card in the selected deck, pre-tagged with all selected tags",
                                            onclick: move |_| {
                                                *crate::app::PENDING_NEW_CARD_TAGS.write() = names.clone();
                                                tabs_ctx.open(Route::CardEditorNew { deck_id: d });
                                            },
                                            "+"
                                        }
                                    }
                                }
                            }
                        }
                        if panel_card_decks.len() > 1 {
                            div { class: "km-panel-cardfilter",
                                for (did , dname) in panel_card_decks.iter().cloned() {
                                    {
                                        let is_sel = panel_deck_filter_snap.contains(&did);
                                        rsx! {
                                            button {
                                                class: if is_sel { "km-deck-chip km-deck-chip-active" } else { "km-deck-chip" },
                                                title: "Show only cards in this deck",
                                                onclick: move |_| {
                                                    let mut f = panel_deck_filter.write();
                                                    if f.contains(&did) { f.remove(&did); } else { f.insert(did); }
                                                },
                                                "{dname}"
                                            }
                                        }
                                    }
                                }
                                if !panel_deck_filter_snap.is_empty() {
                                    button {
                                        class: "km-deck-clear",
                                        onclick: move |_| panel_deck_filter.write().clear(),
                                        "Show all"
                                    }
                                }
                            }
                        }
                        if *cards_loading.read() {
                            p { class: "km-panel-hint", "Loading cards…" }
                        } else if cards.read().is_empty() {
                            p { class: "km-panel-hint", "No cards have all of these tags." }
                        } else if panel_cards.is_empty() {
                            p { class: "km-panel-hint", "No cards in the selected deck(s)." }
                        } else {
                            p { class: "km-panel-hint",
                                if panel_deck_filter_snap.is_empty() {
                                    "{panel_cards.len()} card(s) with all selected tags"
                                } else {
                                    "{panel_cards.len()} card(s) with all selected tags, in the filtered deck(s)"
                                }
                            }
                            div { class: "km-card-list",
                                for card in panel_cards.iter().cloned() {
                                    {
                                        let deck_name = deck_names.read().get(&card.deck_id).cloned().unwrap_or_default();
                                        // One number per selected tag, tinted
                                        // to match that tag's chip above.
                                        let nums: Vec<(i32, i64, String)> = multi_sel.iter()
                                            .map(|(tid, tname)| (
                                                sel_tag_hue.get(tid).copied().unwrap_or(0),
                                                positions_snap.get(&(*tid, card.id)).copied().unwrap_or(0),
                                                tname.clone(),
                                            ))
                                            .collect();
                                        rsx! {
                                            KmCardRow {
                                                key: "{card.id}",
                                                card_id: card.id,
                                                name: card.name.clone(),
                                                deck: deck_name,
                                                nums,
                                                tinted: true,
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Rename a tag ----
            if let Some(ren_id) = *renaming.read() {
                {
                    let old_name = tags_snapshot.iter().find(|t| t.id == ren_id)
                        .map(|t| t.name.clone()).unwrap_or_default();
                    rsx! {
                        div { class: "dialog-backdrop",
                            div { class: "dialog",
                                h3 { class: "dialog-title", "Rename tag \u{201c}{old_name}\u{201d}" }
                                input {
                                    class: "km-sidebar-new km-rename-input",
                                    autofocus: true,
                                    placeholder: "Tag name…",
                                    value: "{rename_text}",
                                    oninput: move |e| rename_text.set(e.value()),
                                    onkeydown: move |e| {
                                        match e.key() {
                                            Key::Enter => {
                                                rename_tag_fn(ren_id, rename_text.read().clone());
                                                renaming.set(None);
                                            }
                                            Key::Escape => renaming.set(None),
                                            _ => {}
                                        }
                                    },
                                }
                                div { class: "dialog-actions",
                                    button {
                                        class: "button button-secondary",
                                        onclick: move |_| renaming.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "button",
                                        onclick: move |_| {
                                            rename_tag_fn(ren_id, rename_text.read().clone());
                                            renaming.set(None);
                                        },
                                        "Save"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Confirm: delete a tag ----
            if let Some(del_id) = *confirm_delete.read() {
                {
                    let name = tags_snapshot.iter().find(|t| t.id == del_id)
                        .map(|t| t.name.clone()).unwrap_or_default();
                    rsx! {
                        div { class: "dialog-backdrop",
                            div { class: "dialog",
                                h3 { class: "dialog-title", "Delete tag \u{201c}{name}\u{201d}?" }
                                p { class: "dialog-body",
                                    "This removes the tag from every card it's on (across all decks) and from the map. This cannot be undone."
                                }
                                div { class: "dialog-actions",
                                    button {
                                        class: "button button-secondary",
                                        onclick: move |_| confirm_delete.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "button button-danger",
                                        onclick: move |_| { delete_tag_fn(del_id); confirm_delete.set(None); },
                                        "Delete tag"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Confirm: detach a tag from all cards ----
            if let Some(det_id) = *confirm_detach.read() {
                {
                    let name = tags_snapshot.iter().find(|t| t.id == det_id)
                        .map(|t| t.name.clone()).unwrap_or_default();
                    rsx! {
                        div { class: "dialog-backdrop",
                            div { class: "dialog",
                                h3 { class: "dialog-title", "Detach all cards from \u{201c}{name}\u{201d}?" }
                                p { class: "dialog-body",
                                    "This removes the tag from every card it's on (across all decks). The tag and its place on the map are kept — you can re-tag cards later."
                                }
                                div { class: "dialog-actions",
                                    button {
                                        class: "button button-secondary",
                                        onclick: move |_| confirm_detach.set(None),
                                        "Cancel"
                                    }
                                    button {
                                        class: "button button-danger",
                                        onclick: move |_| { detach_cards_fn(det_id); confirm_detach.set(None); },
                                        "Detach cards"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Confirm: delete all orphan tags ----
            if *confirm_orphans.read() {
                div { class: "dialog-backdrop",
                    div { class: "dialog",
                        h3 { class: "dialog-title", "Delete orphan tags?" }
                        p { class: "dialog-body",
                            if orphan_count == 0 {
                                "There are no orphan tags — every tag belongs to at least one deck or card."
                            } else if orphan_count == 1 {
                                "1 tag belongs to no deck and no card. It will be deleted everywhere, including the map. This cannot be undone."
                            } else {
                                "{orphan_count} tags belong to no deck and no card. They will be deleted everywhere, including the map. This cannot be undone."
                            }
                        }
                        div { class: "dialog-actions",
                            button {
                                class: "button button-secondary",
                                onclick: move |_| confirm_orphans.set(false),
                                if orphan_count == 0 { "Close" } else { "Cancel" }
                            }
                            if orphan_count > 0 {
                                button {
                                    class: "button button-danger",
                                    onclick: delete_orphan_tags,
                                    if orphan_count == 1 { "Delete 1 tag" } else { "Delete {orphan_count} tags" }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Confirm: clear the whole map ----
            if *confirm_clear.read() {
                div { class: "dialog-backdrop",
                    div { class: "dialog",
                        h3 { class: "dialog-title", "Clear the entire map?" }
                        p { class: "dialog-body",
                            "Every tag will be removed from the canvas and all connections deleted. Your tags and cards stay intact. This cannot be undone."
                        }
                        div { class: "dialog-actions",
                            button {
                                class: "button button-secondary",
                                onclick: move |_| confirm_clear.set(false),
                                "Cancel"
                            }
                            button {
                                class: "button button-danger",
                                onclick: clear_map,
                                "Clear map"
                            }
                        }
                    }
                }
            }

        }
    }
}
