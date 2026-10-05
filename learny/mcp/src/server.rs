//! The Learny MCP server: read and write tools over the desktop app's database.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ErrorData, Implementation, InitializeResult, ServerCapabilities};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::{Deserialize, Serialize};

use domain_flashcard::Block;

use crate::db::Db;

#[derive(Clone)]
pub struct LearnyServer {
    db: Arc<Db>,
    // Read by the code `#[tool_handler]` generates, which dead-code analysis
    // does not see through.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

// Canvas geometry, mirrored from the knowledge-map UI's tidy layout so a tag
// placed from here lands on the same grid the app would use. Duplicated rather
// than shared because the UI crate is a Dioxus frontend this binary must not
// depend on; keep in sync with `ui/src/components/knowledge_map_page.rs`.
const TIDY_VY: f64 = 92.0;   // vertical gap between depth layers
const TIDY_GAP: f64 = 30.0;  // min horizontal gap between node boxes

/// Approximate on-canvas node width from its label length, clamped to a sane
/// range — the UI's own formula.
fn node_width(name: &str) -> f64 {
    (name.chars().count() as f64 * 7.0 + 16.0).clamp(48.0, 230.0)
}

/// Application errors reach the client as tool errors rather than killing the
/// connection.
fn fail(context: &str, e: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(format!("{context}: {e}"), None)
}

// ---------------------------------------------------------------------------
// Tool inputs and outputs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeckSummary {
    pub id: i64,
    pub name: String,
    /// Number of cards in the deck.
    pub card_count: u32,
    /// Unix timestamp (seconds) when the deck was created.
    pub created_at: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagSummary {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListTagsParams {
    /// Restrict to tags used in this deck. Omit for every tag in the collection.
    #[serde(default)]
    pub deck_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListCardsParams {
    /// The deck whose cards to list. Get the id from `list_decks`.
    pub deck_id: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CardSummary {
    pub id: i64,
    pub name: String,
    /// Names of the tags on this card.
    pub tags: Vec<String>,
    /// How many times the card has been answered.
    pub times_seen: u32,
    /// How many of those answers were correct.
    pub times_correct: u32,
    /// Study progress, 0–100.
    pub progress_percent: u8,
    /// Unix timestamp (seconds) when the card was created.
    pub created_at: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateDeckParams {
    /// Name for the new deck. Must not be blank.
    pub name: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateTagParams {
    /// Name for the new tag. Must not be blank.
    pub name: String,
    /// Also attach the tag to this deck, so it shows under that deck's filter.
    #[serde(default)]
    pub deck_id: Option<i64>,
    /// Also link the new tag into the knowledge map under this existing tag,
    /// and place it on the canvas beneath that parent.
    #[serde(default)]
    pub parent_tag_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateCardParams {
    /// Deck to create the card in. Get the id from `list_decks`.
    pub deck_id: i64,
    /// Card title, shown in the deck listing. Must not be blank.
    pub name: String,
    /// Front (question) text. Omit for an empty front.
    #[serde(default)]
    pub front: Option<String>,
    /// Back (answer) text. Omit for an empty back.
    #[serde(default)]
    pub back: Option<String>,
    /// Tag names for the card. Existing tags are matched case-insensitively;
    /// names that don't exist yet are created.
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreatedDeck {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreatedTag {
    pub id: i64,
    pub name: String,
    /// The deck it was attached to, if any.
    pub deck_id: Option<i64>,
    /// The tag it was linked under in the knowledge map, if any.
    pub parent_id: Option<i64>,
    pub parent_name: Option<String>,
    /// Whether it was given a position on the map canvas. Only true when it was
    /// linked under a parent that is itself placed.
    pub placed: bool,
    pub pos_x: Option<f64>,
    pub pos_y: Option<f64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CreatedCard {
    pub id: i64,
    pub deck_id: i64,
    pub name: String,
    /// Tag names actually on the card after creation.
    pub tags: Vec<String>,
    /// True when front text was stored.
    pub has_front: bool,
    /// True when back text was stored.
    pub has_back: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateCardParams {
    /// The card to edit. Get the id from `list_cards`.
    pub card_id: i64,
    /// New card title. Omit to leave the title alone; must not be blank.
    #[serde(default)]
    pub name: Option<String>,
    /// Replacement front (question) text, full markdown + Mathjax. Omit to
    /// leave the front alone; pass an empty string to clear its text.
    #[serde(default)]
    pub front: Option<String>,
    /// Replacement back (answer) text. Omit to leave the back alone; pass an
    /// empty string to clear its text.
    #[serde(default)]
    pub back: Option<String>,
    /// The card's complete tag list after the edit — tags left out of it are
    /// removed from the card. Omit to leave tags alone; pass an empty list to
    /// take every tag off. Names that don't exist yet are created.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UpdatedCard {
    pub id: i64,
    pub deck_id: i64,
    /// The title after the edit.
    pub name: String,
    /// Tag names actually on the card after the edit.
    pub tags: Vec<String>,
    /// The front's text after the edit.
    pub front_text: String,
    /// The back's text after the edit.
    pub back_text: String,
    /// Which fields this call actually changed: any of "name", "front",
    /// "back", "tags". Empty when the card already read that way.
    pub changed: Vec<String>,
    /// How many image/audio/video/file blocks were kept on the sides this call
    /// rewrote. Editing text never removes the card's media.
    pub media_kept: usize,
}

/// MCP requires every tool's `outputSchema` to be an object, so each result
/// list travels inside a named field rather than as a bare JSON array.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PingResult {
    /// Always "ok" when the server is up and the database opened.
    pub status: String,
    pub server_version: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DeckListResult {
    pub decks: Vec<DeckSummary>,
    /// Number of decks returned.
    pub count: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagListResult {
    pub tags: Vec<TagSummary>,
    /// Number of tags returned.
    pub count: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CardListResult {
    /// The deck these cards belong to.
    pub deck_id: i64,
    pub deck_name: String,
    pub cards: Vec<CardSummary>,
    /// Number of cards returned.
    pub count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GetCardParams {
    /// The card to read. Get the id from `list_cards`.
    pub card_id: i64,
}

/// One piece of card content. Text blocks carry `text`; media blocks carry
/// `src`, a path relative to the app's data directory (the file itself is not
/// readable through this server).
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CardBlock {
    /// One of "text", "image", "audio", "video", "file".
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
}

impl From<&Block> for CardBlock {
    fn from(b: &Block) -> Self {
        let (text, src) = match b {
            Block::Text { value } => (Some(value.clone()), None),
            other => (None, other.file_path().map(str::to_string)),
        };
        CardBlock { kind: b.block_type().to_string(), text, src }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CardDetail {
    pub id: i64,
    pub deck_id: i64,
    pub deck_name: String,
    pub name: String,
    /// Names of the tags on this card.
    pub tags: Vec<String>,
    /// Front (question) side, block by block.
    pub front: Vec<CardBlock>,
    /// Back (answer) side, block by block.
    pub back: Vec<CardBlock>,
    /// The front's text blocks joined with blank lines — the question as plain
    /// text, with media blocks left out.
    pub front_text: String,
    /// The back's text blocks joined with blank lines.
    pub back_text: String,
    pub times_seen: u32,
    pub times_correct: u32,
    /// Study progress, 0–100.
    pub progress_percent: u8,
    /// The user's own note on what they found easy about this card.
    pub what_was_easy: Option<String>,
    /// The user's own note on what they found difficult.
    pub what_was_difficult: Option<String>,
    /// Unix timestamp (seconds) when the card was created.
    pub created_at: i64,
}

/// A tag as it sits in the knowledge map.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagNode {
    pub id: i64,
    pub name: String,
    /// Ids of the tags this one sits under. The map is a DAG, so a tag can
    /// have several parents (e.g. "control flow" under both "rust" and "asm").
    pub parents: Vec<i64>,
    /// Ids of the tags directly under this one.
    pub children: Vec<i64>,
    /// True once the tag has been given a position on the map canvas; unplaced
    /// tags still take part in the hierarchy but sit in the app's sidebar.
    pub placed: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagLink {
    pub parent_id: i64,
    pub parent_name: String,
    pub child_id: i64,
    pub child_name: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagMapResult {
    pub tags: Vec<TagNode>,
    /// Every parent → child link in the map.
    pub links: Vec<TagLink>,
    /// The same hierarchy as an indented outline, roots first, with tags that
    /// have no links at all listed at the end. A tag with several parents
    /// appears under each of them.
    pub outline: String,
    pub tag_count: usize,
    pub link_count: usize,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TagLinkParams {
    /// The broader tag, e.g. "rust". Get ids from `list_tags` or `get_tag_map`.
    pub parent_tag_id: i64,
    /// The narrower tag that sits under it, e.g. "ownership".
    pub child_tag_id: i64,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagLinkResult {
    pub parent_id: i64,
    pub parent_name: String,
    pub child_id: i64,
    pub child_name: String,
    /// False when the link was already there and nothing changed.
    pub created: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct TagUnlinkResult {
    pub parent_id: i64,
    pub parent_name: String,
    pub child_id: i64,
    pub child_name: String,
    /// False when there was no such link and nothing changed.
    pub removed: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AssignTagsParams {
    /// The deck. Get the id from `list_decks`.
    pub deck_id: i64,
    /// Ids of existing tags. Get them from `list_tags` or
    /// `get_tag_map`.
    pub tag_ids: Vec<i64>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AssignedTag {
    pub id: i64,
    pub name: String,
    /// False when the tag was already attached to the deck and nothing changed.
    pub added: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct AssignTagsResult {
    pub deck_id: i64,
    pub deck_name: String,
    pub tags: Vec<AssignedTag>,
    /// Number of tags newly attached by this call.
    pub added_count: usize,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UnassignedTag {
    pub id: i64,
    pub name: String,
    /// False when the tag was not attached to the deck and nothing changed.
    pub removed: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct UnassignTagsResult {
    pub deck_id: i64,
    pub deck_name: String,
    pub tags: Vec<UnassignedTag>,
    /// Number of tags detached by this call.
    pub removed_count: usize,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router]
impl LearnyServer {
    pub fn new(db: Db) -> Self {
        Self { db: Arc::new(db), tool_router: Self::tool_router() }
    }

    #[tool(
        name = "ping",
        description = "Health check. Confirms the Learny MCP server is running and can open the database.",
        annotations(
            title = "Ping Learny",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn ping(&self) -> Json<PingResult> {
        Json(PingResult {
            status: "ok".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    #[tool(
        name = "list_decks",
        description = "List every deck in the Learny collection, with its card count.",
        annotations(
            title = "List decks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_decks(&self) -> Result<Json<DeckListResult>, ErrorData> {
        let decks = self.db.flashcard.get_decks().map_err(|e| fail("listing decks", e))?;
        let decks: Vec<DeckSummary> = decks
            .into_iter()
            .map(|d| DeckSummary {
                id: d.id,
                name: d.name,
                card_count: d.card_count,
                created_at: d.created_at,
            })
            .collect();
        Ok(Json(DeckListResult { count: decks.len(), decks }))
    }

    #[tool(
        name = "list_tags",
        description = "List tags — every tag in the collection, or a single deck's tags when deck_id is given. A deck's tags are those used by its cards plus any explicitly attached to the deck.",
        annotations(
            title = "List tags",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_tags(
        &self,
        Parameters(ListTagsParams { deck_id }): Parameters<ListTagsParams>,
    ) -> Result<Json<TagListResult>, ErrorData> {
        let tags = match deck_id {
            // `tags_for_deck` only finds tags reachable through the deck's
            // cards, so tags merely attached to the deck (what `create_tag`
            // with a deck_id does) would be invisible. Union the two.
            Some(id) => {
                let mut tags =
                    self.db.tagging.tags_for_deck(id).map_err(|e| fail("listing deck tags", e))?;
                let attached: HashSet<i64> = self
                    .db
                    .tagging
                    .get_tag_deck_pairs()
                    .map_err(|e| fail("loading deck tag assignments", e))?
                    .into_iter()
                    .filter(|(_, deck)| *deck == id)
                    .map(|(tag, _)| tag)
                    .collect();
                if !attached.is_empty() {
                    let known: HashSet<i64> = tags.iter().map(|t| t.id).collect();
                    let extra = self
                        .db
                        .tagging
                        .get_all_tags()
                        .map_err(|e| fail("listing tags", e))?
                        .into_iter()
                        .filter(|t| attached.contains(&t.id) && !known.contains(&t.id));
                    tags.extend(extra);
                    tags.sort_by_key(|t| t.name.to_lowercase());
                }
                tags
            }
            None => self.db.tagging.get_all_tags().map_err(|e| fail("listing tags", e))?,
        };
        let tags: Vec<TagSummary> =
            tags.into_iter().map(|t| TagSummary { id: t.id, name: t.name }).collect();
        Ok(Json(TagListResult { count: tags.len(), tags }))
    }

    #[tool(
        name = "list_cards",
        description = "List all cards in a deck with their tags and study progress. Returns card titles and metadata, not the card content.",
        annotations(
            title = "List cards in a deck",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn list_cards(
        &self,
        Parameters(ListCardsParams { deck_id }): Parameters<ListCardsParams>,
    ) -> Result<Json<CardListResult>, ErrorData> {
        // Fail loudly on a bad id rather than returning an empty list that
        // looks like an empty deck. The message carries the ids that do exist,
        // so a caller that guessed can correct itself instead of retrying the
        // same wrong id.
        let deck = match self.db.flashcard.get_deck(deck_id) {
            Ok(deck) => deck,
            Err(e) => {
                let available = match self.db.flashcard.get_decks() {
                    Ok(decks) => decks
                        .iter()
                        .map(|d| format!("{} ({})", d.id, d.name))
                        .collect::<Vec<_>>()
                        .join(", "),
                    // The listing is a convenience; don't mask the real error.
                    Err(e) => format!("<could not list decks: {e}>"),
                };
                return Err(ErrorData::invalid_params(
                    format!(
                        "no deck with id {deck_id}: {e}. Call `list_decks` for the \
                         current ids. Available decks: {available}"
                    ),
                    None,
                ));
            }
        };

        let cards = self.db.flashcard.get_cards(deck_id).map_err(|e| fail("listing cards", e))?;

        // Two queries for the whole deck rather than one per card.
        let tag_names: HashMap<i64, String> = self
            .db
            .tagging
            .tags_for_deck(deck_id)
            .map_err(|e| fail("loading deck tags", e))?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect();
        let mut by_card: HashMap<i64, Vec<String>> = HashMap::new();
        for (card_id, tag_id) in self
            .db
            .tagging
            .card_tag_pairs_for_deck(deck_id)
            .map_err(|e| fail("loading card tags", e))?
        {
            if let Some(name) = tag_names.get(&tag_id) {
                by_card.entry(card_id).or_default().push(name.clone());
            }
        }
        for names in by_card.values_mut() {
            names.sort();
        }

        let cards: Vec<CardSummary> = cards
            .into_iter()
            .map(|c| CardSummary {
                tags: by_card.remove(&c.id).unwrap_or_default(),
                progress_percent: c.progress_percent(),
                id: c.id,
                name: c.name,
                times_seen: c.times_seen,
                times_correct: c.times_correct,
                created_at: c.created_at,
            })
            .collect();
        Ok(Json(CardListResult {
            deck_id,
            deck_name: deck.name,
            count: cards.len(),
            cards,
        }))
    }

    #[tool(
        name = "get_card",
        description = "Read one card in full: its front and back content block by block, its tags, study progress and the user's own notes. Use this after `list_cards`, which returns titles only.",
        annotations(
            title = "Read a card",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_card(
        &self,
        Parameters(GetCardParams { card_id }): Parameters<GetCardParams>,
    ) -> Result<Json<CardDetail>, ErrorData> {
        let card = self.db.flashcard.get_card(card_id).map_err(|e| {
            ErrorData::invalid_params(
                format!(
                    "no card with id {card_id}: {e}. Call `list_cards` for a deck's card ids."
                ),
                None,
            )
        })?;
        // `get_card` already checked the deck is the user's, so this lookup is
        // for the name only.
        let deck_name = self
            .db
            .flashcard
            .get_deck(card.deck_id)
            .map(|d| d.name)
            .map_err(|e| fail("loading the card's deck", e))?;
        let tags: Vec<String> = self
            .db
            .tagging
            .tags_for_card(card_id)
            .map_err(|e| fail("loading card tags", e))?
            .into_iter()
            .map(|t| t.name)
            .collect();

        let progress_percent = card.progress_percent();
        Ok(Json(CardDetail {
            front: card.front_blocks.iter().map(CardBlock::from).collect(),
            back: card.back_blocks.iter().map(CardBlock::from).collect(),
            front_text: plain_text(&card.front_blocks),
            back_text: plain_text(&card.back_blocks),
            id: card.id,
            deck_id: card.deck_id,
            deck_name,
            name: card.name,
            tags,
            times_seen: card.times_seen,
            times_correct: card.times_correct,
            progress_percent,
            what_was_easy: card.what_was_easy,
            what_was_difficult: card.what_was_difficult,
            created_at: card.created_at,
        }))
    }

    #[tool(
        name = "get_tag_map",
        description = "Read the knowledge map: every tag, the parent → child links between them, and an indented outline of the hierarchy. The map is one global structure shared by all decks, and a DAG — a tag can sit under several parents.",
        annotations(
            title = "Read the tag map",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn get_tag_map(&self) -> Result<Json<TagMapResult>, ErrorData> {
        let tags = self.db.tagging.get_all_tags().map_err(|e| fail("listing tags", e))?;
        let edges = self.db.tagging.get_tag_edges().map_err(|e| fail("loading tag links", e))?;
        let names: HashMap<i64, String> =
            tags.iter().map(|t| (t.id, t.name.clone())).collect();

        let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut parents: HashMap<i64, Vec<i64>> = HashMap::new();
        for (parent, child) in &edges {
            // Skip any edge pointing at a tag that no longer exists rather than
            // inventing a name for it.
            if !names.contains_key(parent) || !names.contains_key(child) {
                continue;
            }
            children.entry(*parent).or_default().push(*child);
            parents.entry(*child).or_default().push(*parent);
        }
        for ids in children.values_mut().chain(parents.values_mut()) {
            ids.sort_by_key(|id| names.get(id).map(|n| n.to_lowercase()).unwrap_or_default());
        }

        let mut links: Vec<TagLink> = edges
            .iter()
            .filter(|(p, c)| names.contains_key(p) && names.contains_key(c))
            .map(|(p, c)| TagLink {
                parent_id: *p,
                parent_name: names[p].clone(),
                child_id: *c,
                child_name: names[c].clone(),
            })
            .collect();
        links.sort_by_key(|l| (l.parent_name.to_lowercase(), l.child_name.to_lowercase()));

        // `get_all_tags` returns them by name, which sets the outline's order.
        let ordered: Vec<i64> = tags.iter().map(|t| t.id).collect();
        let outline = outline(&ordered, &children, &parents, &names);

        let nodes: Vec<TagNode> = tags
            .into_iter()
            .map(|t| TagNode {
                parents: parents.get(&t.id).cloned().unwrap_or_default(),
                children: children.get(&t.id).cloned().unwrap_or_default(),
                placed: t.pos_x.is_some() && t.pos_y.is_some(),
                id: t.id,
                name: t.name,
            })
            .collect();

        Ok(Json(TagMapResult {
            tag_count: nodes.len(),
            link_count: links.len(),
            tags: nodes,
            links,
            outline,
        }))
    }

    // -----------------------------------------------------------------------
    // Writes
    // -----------------------------------------------------------------------

    #[tool(
        name = "create_deck",
        description = "Create a new, empty deck.",
        annotations(
            title = "Create a deck",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn create_deck(
        &self,
        Parameters(CreateDeckParams { name }): Parameters<CreateDeckParams>,
    ) -> Result<Json<CreatedDeck>, ErrorData> {
        let deck = self
            .db
            .flashcard
            .create_deck(&name)
            .map_err(|e| ErrorData::invalid_params(format!("could not create deck: {e}"), None))?;
        Ok(Json(CreatedDeck { id: deck.id, name: deck.name }))
    }

    #[tool(
        name = "create_tag",
        description = "Create a tag. Use very secific Tag so not algbra, functions, exponential but only exponantial. Only one specific tag! Optionally attach it to a deck so it shows under that deck's filter, and/or link it into the knowledge map under an existing parent tag. Linking under a parent that is already on the canvas also places the new tag below it, so it shows up on the map straight away; under an unplaced parent the link is made but the tag stays in the sidebar.",
        annotations(
            title = "Create a tag",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn create_tag(
        &self,
        Parameters(CreateTagParams { name, deck_id, parent_tag_id }): Parameters<CreateTagParams>,
    ) -> Result<Json<CreatedTag>, ErrorData> {
        // Resolve the parent before creating anything, so a bad id fails the
        // call outright instead of leaving a stray tag behind. `tag_edge` has
        // no FKs, so nothing else would catch it.
        let tags = self.db.tagging.get_all_tags().map_err(|e| fail("listing tags", e))?;
        let parent = match parent_tag_id {
            Some(pid) => Some(
                tags.iter()
                    .find(|t| t.id == pid)
                    .ok_or_else(|| {
                        ErrorData::invalid_params(
                            format!("parent_tag_id {pid} is not an existing tag"),
                            None,
                        )
                    })?
                    .clone(),
            ),
            None => None,
        };

        let tag = self
            .db
            .tagging
            .create_tag(&name)
            .map_err(|e| ErrorData::invalid_params(format!("could not create tag: {e}"), None))?;
        if let Some(deck_id) = deck_id {
            self.db
                .tagging
                .assign_tag_to_deck(tag.id, deck_id)
                .map_err(|e| fail("attaching tag to deck", e))?;
        }

        let mut pos = None;
        if let Some(parent) = &parent {
            // A brand-new tag has no children, so this link can never cycle.
            self.db.tagging.add_tag_edge(parent.id, tag.id).map_err(|e| {
                ErrorData::invalid_params(format!("could not link tags: {e}"), None)
            })?;

            // Place it one layer below the parent, clear of whichever siblings
            // are already on the canvas. An unplaced parent is left alone:
            // placing the child there would draw a node whose only link points
            // at nothing visible. The map's Tidy button re-flows either way.
            if let (Some(px), Some(py)) = (parent.pos_x, parent.pos_y) {
                let edges = self
                    .db
                    .tagging
                    .get_tag_edges()
                    .map_err(|e| fail("loading tag links", e))?;
                let sibling_right = tags
                    .iter()
                    .filter(|t| edges.contains(&(parent.id, t.id)))
                    .filter_map(|t| t.pos_x.map(|x| x + node_width(&t.name)))
                    .fold(f64::NEG_INFINITY, f64::max);
                let x = if sibling_right.is_finite() { sibling_right + TIDY_GAP } else { px };
                let y = py + TIDY_VY;
                self.db
                    .tagging
                    .set_tag_position(tag.id, x, y)
                    .map_err(|e| fail("placing the tag on the map", e))?;
                pos = Some((x, y));
            }
        }

        Ok(Json(CreatedTag {
            id: tag.id,
            name: tag.name,
            deck_id,
            parent_id: parent.as_ref().map(|p| p.id),
            parent_name: parent.as_ref().map(|p| p.name.clone()),
            placed: pos.is_some(),
            pos_x: pos.map(|(x, _)| x),
            pos_y: pos.map(|(_, y)| y),
        }))
    }

    #[tool(
        name = "create_card",
        description = "Create a card in a deck, with optional front/back text and tags. Use full markdown + Mathjax. Tags that don't exist yet are created.",
        annotations(
            title = "Create a card",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    fn create_card(
        &self,
        Parameters(CreateCardParams { deck_id, name, front, back, tags }): Parameters<
            CreateCardParams,
        >,
    ) -> Result<Json<CreatedCard>, ErrorData> {
        let card = self
            .db
            .flashcard
            .add_card(deck_id, &name)
            .map_err(|e| ErrorData::invalid_params(format!("could not create card: {e}"), None))?;

        // Blank text is the same as no text — don't store an empty block.
        let to_blocks = |t: Option<String>| -> Vec<Block> {
            match t {
                Some(v) if !v.trim().is_empty() => vec![Block::Text { value: v }],
                _ => Vec::new(),
            }
        };
        let front_blocks = to_blocks(front);
        let back_blocks = to_blocks(back);
        let (has_front, has_back) = (!front_blocks.is_empty(), !back_blocks.is_empty());

        if has_front || has_back {
            self.db
                .flashcard
                .save_blocks(card.id, front_blocks, back_blocks)
                .map_err(|e| fail("saving card content", e))?;
        }

        let applied = if tags.is_empty() {
            Vec::new()
        } else {
            self.db
                .tagging
                .sync_tags_for_card(card.id, tags)
                .map_err(|e| fail("tagging card", e))?
                .into_iter()
                .map(|t| t.name)
                .collect()
        };

        Ok(Json(CreatedCard {
            id: card.id,
            deck_id,
            name: card.name,
            tags: applied,
            has_front,
            has_back,
        }))
    }

    #[tool(
        name = "update_card",
        description = "Edit an existing card: its title, front/back text, or tags. Use full markdown + Mathjax. Every field is optional — one left out is kept as it was, so a call can change the front alone. Text replaces that side's old text and leaves its images and other media in place; `tags` replaces the whole tag list, creating tags that don't exist yet. Read the card with `get_card` first if you mean to build on what it says.",
        annotations(
            title = "Edit a card",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn update_card(
        &self,
        Parameters(UpdateCardParams { card_id, name, front, back, tags }): Parameters<
            UpdateCardParams,
        >,
    ) -> Result<Json<UpdatedCard>, ErrorData> {
        if name.is_none() && front.is_none() && back.is_none() && tags.is_none() {
            return Err(ErrorData::invalid_params(
                "nothing to update: pass at least one of `name`, `front`, `back` or `tags`."
                    .to_string(),
                None,
            ));
        }

        let card = self.db.flashcard.get_card(card_id).map_err(|e| {
            ErrorData::invalid_params(
                format!(
                    "no card with id {card_id}: {e}. Call `list_cards` for a deck's card ids."
                ),
                None,
            )
        })?;

        let mut changed: Vec<String> = Vec::new();

        let final_name = match name {
            // A rename to the title it already has is not a change to report.
            Some(n) if n.trim() == card.name => card.name.clone(),
            Some(n) => {
                let renamed = self.db.flashcard.rename_card(card_id, &n).map_err(|e| {
                    ErrorData::invalid_params(format!("could not rename card: {e}"), None)
                })?;
                changed.push("name".to_string());
                renamed.name
            }
            None => card.name.clone(),
        };

        // `save_blocks` writes both sides at once, so the side this call does
        // not touch is written back exactly as it was read.
        let mut front_blocks = card.front_blocks.clone();
        let mut back_blocks = card.back_blocks.clone();
        let mut media_kept = 0;
        if let Some(text) = &front {
            front_blocks = replace_text(&card.front_blocks, text);
            media_kept += media_count(&front_blocks);
        }
        if let Some(text) = &back {
            back_blocks = replace_text(&card.back_blocks, text);
            media_kept += media_count(&back_blocks);
        }
        if front.is_some() || back.is_some() {
            let front_moved = front_blocks != card.front_blocks;
            let back_moved = back_blocks != card.back_blocks;
            if front_moved || back_moved {
                self.db
                    .flashcard
                    .save_blocks(card_id, front_blocks.clone(), back_blocks.clone())
                    .map_err(|e| fail("saving card content", e))?;
                if front_moved {
                    changed.push("front".to_string());
                }
                if back_moved {
                    changed.push("back".to_string());
                }
            }
        }

        let final_tags: Vec<String> = match tags {
            Some(wanted) => {
                let before: Vec<String> = self
                    .db
                    .tagging
                    .tags_for_card(card_id)
                    .map_err(|e| fail("loading card tags", e))?
                    .into_iter()
                    .map(|t| t.name)
                    .collect();
                let after: Vec<String> = self
                    .db
                    .tagging
                    .sync_tags_for_card(card_id, wanted)
                    .map_err(|e| fail("tagging card", e))?
                    .into_iter()
                    .map(|t| t.name)
                    .collect();
                if after != before {
                    changed.push("tags".to_string());
                }
                after
            }
            None => self
                .db
                .tagging
                .tags_for_card(card_id)
                .map_err(|e| fail("loading card tags", e))?
                .into_iter()
                .map(|t| t.name)
                .collect(),
        };

        Ok(Json(UpdatedCard {
            id: card_id,
            deck_id: card.deck_id,
            name: final_name,
            tags: final_tags,
            front_text: plain_text(&front_blocks),
            back_text: plain_text(&back_blocks),
            changed,
            media_kept,
        }))
    }

    #[tool(
        name = "assign_tags_to_deck",
        description = "Attach existing tags to a deck, so they show under that deck's filter even before any of its cards use them. Takes tag ids — create missing tags with `create_tag` first. Tags already attached are left as they are.",
        annotations(
            title = "Assign tags to a deck",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn assign_tags_to_deck(
        &self,
        Parameters(AssignTagsParams { deck_id, tag_ids }): Parameters<AssignTagsParams>,
    ) -> Result<Json<AssignTagsResult>, ErrorData> {
        let (deck_name, names, unique, attached) = self.resolve_deck_tags(deck_id, &tag_ids)?;

        let mut tags = Vec::with_capacity(unique.len());
        for id in unique {
            let added = !attached.contains(&id);
            if added {
                self.db
                    .tagging
                    .assign_tag_to_deck(id, deck_id)
                    .map_err(|e| fail("attaching tag to deck", e))?;
            }
            tags.push(AssignedTag { id, name: names[&id].clone(), added });
        }

        Ok(Json(AssignTagsResult {
            deck_id,
            deck_name,
            added_count: tags.iter().filter(|t| t.added).count(),
            tags,
        }))
    }

    #[tool(
        name = "unassign_tags_from_deck",
        description = "Detach tags from a deck, so they no longer show under that deck's filter through the deck itself. The tags, their knowledge-map links and the cards that carry them are untouched — a tag still used by a card in the deck keeps showing up via that card. Tags not attached to the deck are skipped.",
        annotations(
            title = "Unassign tags from a deck",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn unassign_tags_from_deck(
        &self,
        Parameters(AssignTagsParams { deck_id, tag_ids }): Parameters<AssignTagsParams>,
    ) -> Result<Json<UnassignTagsResult>, ErrorData> {
        let (deck_name, names, unique, attached) = self.resolve_deck_tags(deck_id, &tag_ids)?;

        let mut tags = Vec::with_capacity(unique.len());
        for id in unique {
            let removed = attached.contains(&id);
            if removed {
                self.db
                    .tagging
                    .unassign_tag_from_deck(id, deck_id)
                    .map_err(|e| fail("detaching tag from deck", e))?;
            }
            tags.push(UnassignedTag { id, name: names[&id].clone(), removed });
        }

        Ok(Json(UnassignTagsResult {
            deck_id,
            deck_name,
            removed_count: tags.iter().filter(|t| t.removed).count(),
            tags,
        }))
    }

    #[tool(
        name = "link_tags",
        description = "Put one tag under another in the knowledge map, e.g. \"ownership\" under \"rust\". A tag may have several parents; links that would form a cycle are rejected. Does nothing if the link already exists.",
        annotations(
            title = "Link two tags",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn link_tags(
        &self,
        Parameters(TagLinkParams { parent_tag_id, child_tag_id }): Parameters<TagLinkParams>,
    ) -> Result<Json<TagLinkResult>, ErrorData> {
        let names = self.tag_names()?;
        let parent_name = tag_name(&names, parent_tag_id, "parent_tag_id")?;
        let child_name = tag_name(&names, child_tag_id, "child_tag_id")?;

        // The insert is an INSERT OR IGNORE, so check first to report honestly
        // whether this call changed anything.
        let existed = self
            .db
            .tagging
            .get_tag_edges()
            .map_err(|e| fail("loading tag links", e))?
            .contains(&(parent_tag_id, child_tag_id));
        if !existed {
            self.db.tagging.add_tag_edge(parent_tag_id, child_tag_id).map_err(|e| {
                ErrorData::invalid_params(format!("could not link tags: {e}"), None)
            })?;
        }

        Ok(Json(TagLinkResult {
            parent_id: parent_tag_id,
            parent_name,
            child_id: child_tag_id,
            child_name,
            created: !existed,
        }))
    }

    #[tool(
        name = "unlink_tags",
        description = "Remove a parent → child link from the knowledge map. The tags themselves and the cards on them are untouched.",
        annotations(
            title = "Unlink two tags",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    fn unlink_tags(
        &self,
        Parameters(TagLinkParams { parent_tag_id, child_tag_id }): Parameters<TagLinkParams>,
    ) -> Result<Json<TagUnlinkResult>, ErrorData> {
        let names = self.tag_names()?;
        let parent_name = tag_name(&names, parent_tag_id, "parent_tag_id")?;
        let child_name = tag_name(&names, child_tag_id, "child_tag_id")?;

        let existed = self
            .db
            .tagging
            .get_tag_edges()
            .map_err(|e| fail("loading tag links", e))?
            .contains(&(parent_tag_id, child_tag_id));
        if existed {
            self.db
                .tagging
                .remove_tag_edge(parent_tag_id, child_tag_id)
                .map_err(|e| fail("unlinking tags", e))?;
        }

        Ok(Json(TagUnlinkResult {
            parent_id: parent_tag_id,
            parent_name,
            child_id: child_tag_id,
            child_name,
            removed: existed,
        }))
    }
}

/// `tool_handler` routes `tools/list` and `tools/call` through `tool_router`;
/// only the handshake response is spelled out here.
#[tool_handler]
impl ServerHandler for LearnyServer {
    fn get_info(&self) -> InitializeResult {
        InitializeResult::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("learny-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Access to the Learny flashcard collection. \
                 Start with `list_decks` to get deck ids, then `list_cards` for a deck's \
                 cards and their tags, and `get_card` for one card's actual content; \
                 `list_tags` lists tags globally or per deck. \
                 `get_tag_map` shows the knowledge map — one global hierarchy of tags \
                 (a DAG: a tag can sit under several parents) shared by every deck — \
                 and `link_tags`/`unlink_tags` edit it. \
                 `create_deck`, `create_card` and `create_tag` write to the collection — \
                 `create_card` takes front/back text and tag names, creating any tag that \
                 doesn't exist yet — and `update_card` edits a card that already exists, \
                 changing only the fields it is given.",
            )
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

impl LearnyServer {
    /// Tag ids to names, used to validate ids and to label results. The
    /// `tag_edge` table has no foreign keys, so an unchecked id would quietly
    /// create a link to a tag that does not exist.
    /// Shared prelude of the deck-tag tools: the deck's name, every tag's
    /// name, the requested ids deduplicated in order, and the ids currently
    /// attached to the deck. `tag_deck` has no FKs, so every id is validated
    /// here before anything is written; a bad id then fails the whole call
    /// instead of half-applying it.
    fn resolve_deck_tags(
        &self,
        deck_id: i64,
        tag_ids: &[i64],
    ) -> Result<(String, HashMap<i64, String>, Vec<i64>, HashSet<i64>), ErrorData> {
        let deck = self.db.flashcard.get_deck(deck_id).map_err(|e| {
            ErrorData::invalid_params(
                format!("no deck with id {deck_id}: {e}. Call `list_decks` for the current ids."),
                None,
            )
        })?;
        let names = self.tag_names()?;
        let mut unique: Vec<i64> = Vec::new();
        for &id in tag_ids {
            tag_name(&names, id, "tag_ids")?;
            if !unique.contains(&id) {
                unique.push(id);
            }
        }

        // The writes are INSERT OR IGNORE / plain DELETE, so check first to
        // report honestly which tags a call actually changed.
        let attached: HashSet<i64> = self
            .db
            .tagging
            .get_tag_deck_pairs()
            .map_err(|e| fail("loading deck tag assignments", e))?
            .into_iter()
            .filter(|&(_, d)| d == deck_id)
            .map(|(t, _)| t)
            .collect();

        Ok((deck.name, names, unique, attached))
    }

    fn tag_names(&self) -> Result<HashMap<i64, String>, ErrorData> {
        Ok(self
            .db
            .tagging
            .get_all_tags()
            .map_err(|e| fail("listing tags", e))?
            .into_iter()
            .map(|t| (t.id, t.name))
            .collect())
    }
}

fn tag_name(names: &HashMap<i64, String>, id: i64, role: &str) -> Result<String, ErrorData> {
    names.get(&id).cloned().ok_or_else(|| {
        ErrorData::invalid_params(
            format!(
                "no tag with id {id} ({role}). Call `list_tags` or `get_tag_map` \
                 for the current ids."
            ),
            None,
        )
    })
}

/// How many of these blocks carry a file rather than text.
fn media_count(blocks: &[Block]) -> usize {
    blocks.iter().filter(|b| !matches!(b, Block::Text { .. })).count()
}

/// One side's blocks with its text replaced by `text`. Media blocks are kept
/// in order — editing a card's words is not a reason to drop its images — and
/// the new text takes the place of the side's first text block, or goes last
/// when it had none. Blank text leaves the side with its media alone.
fn replace_text(blocks: &[Block], text: &str) -> Vec<Block> {
    let new_block =
        (!text.trim().is_empty()).then(|| Block::Text { value: text.to_string() });
    let mut out = Vec::with_capacity(blocks.len() + 1);
    let mut placed = false;
    for block in blocks {
        match block {
            Block::Text { .. } => {
                if !placed {
                    placed = true;
                    out.extend(new_block.clone());
                }
            }
            other => out.push(other.clone()),
        }
    }
    if !placed {
        out.extend(new_block);
    }
    out
}

/// The text blocks of one side, joined with blank lines. Media blocks are left
/// out — they appear in the block list with their stored path.
fn plain_text(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            Block::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Renders the map as an indented outline: every root with its descendants,
/// then the tags that take part in no link at all. `ordered` sets the order
/// tags are considered in (the repository returns them by name).
fn outline(
    ordered: &[i64],
    children: &HashMap<i64, Vec<i64>>,
    parents: &HashMap<i64, Vec<i64>>,
    names: &HashMap<i64, String>,
) -> String {
    fn walk(
        out: &mut String,
        node: i64,
        depth: usize,
        children: &HashMap<i64, Vec<i64>>,
        names: &HashMap<i64, String>,
        path: &mut Vec<i64>,
    ) {
        let indent = "  ".repeat(depth);
        let name = names.get(&node).map(String::as_str).unwrap_or("<unknown tag>");
        // The service rejects cycles when linking, but an outline that can
        // recurse forever is not worth the risk.
        if path.contains(&node) {
            out.push_str(&format!("{indent}- {name} (loop)\n"));
            return;
        }
        out.push_str(&format!("{indent}- {name}\n"));
        path.push(node);
        for child in children.get(&node).into_iter().flatten() {
            walk(out, *child, depth + 1, children, names, path);
        }
        path.pop();
    }

    let mut out = String::new();
    for id in ordered {
        // Roots only: something with a parent is printed under it instead, and
        // a tag with no links at all is listed separately below.
        if parents.contains_key(id) || !children.contains_key(id) {
            continue;
        }
        walk(&mut out, *id, 0, children, names, &mut Vec::new());
    }

    let loose: Vec<&str> = ordered
        .iter()
        .filter(|id| !parents.contains_key(id) && !children.contains_key(id))
        .filter_map(|id| names.get(id).map(String::as_str))
        .collect();
    if !loose.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("Unlinked tags: {}\n", loose.join(", ")));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(v: &str) -> Block {
        Block::Text { value: v.to_string() }
    }
    fn image(src: &str) -> Block {
        Block::Image { src: src.to_string(), scale: None }
    }

    #[test]
    fn new_text_takes_the_place_of_the_old() {
        let side = vec![text("old")];
        assert_eq!(replace_text(&side, "new"), vec![text("new")]);
    }

    #[test]
    fn media_survives_a_text_edit() {
        let side = vec![text("old"), image("a.png")];
        assert_eq!(replace_text(&side, "new"), vec![text("new"), image("a.png")]);
    }

    #[test]
    fn text_goes_last_on_a_side_that_had_none() {
        let side = vec![image("a.png")];
        assert_eq!(replace_text(&side, "new"), vec![image("a.png"), text("new")]);
    }

    #[test]
    fn several_text_blocks_collapse_into_one() {
        let side = vec![text("one"), image("a.png"), text("two")];
        assert_eq!(replace_text(&side, "new"), vec![text("new"), image("a.png")]);
    }

    #[test]
    fn blank_text_clears_the_side_but_not_its_media() {
        let side = vec![text("old"), image("a.png")];
        assert_eq!(replace_text(&side, "  "), vec![image("a.png")]);
        assert_eq!(replace_text(&[text("old")], ""), Vec::<Block>::new());
    }

    #[test]
    fn media_is_counted_apart_from_text() {
        assert_eq!(media_count(&[text("t"), image("a.png"), image("b.png")]), 2);
        assert_eq!(media_count(&[text("t")]), 0);
    }
}
