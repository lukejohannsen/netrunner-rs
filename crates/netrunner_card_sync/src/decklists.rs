//! A published decklist, fetched from NetrunnerDB by its link.
//!
//! **One request, by uuid, over v3** (Phase 7 §10 Stage 6, 30 September
//! 2026). NetrunnerDB's v3 API serves a published decklist at
//! `/api/v3/public/decklists/{uuid}` with its name, its author's user
//! name, its notes as HTML, the identity's card id and `card_slots` — each
//! card's v3 id with its copies, the identity among them. The catalog is
//! v3's (NSG pool Stage 0d), so every id here is one `deck_builder` can
//! match without a title or a printing code in between; what the catalog
//! does not know is reported by id, never guessed at.
//!
//! **What a link is, read off the site on 30 September 2026.** A decklist's
//! page is `netrunnerdb.com/<lang>/decklist/<uuid>/<slug>`, and the slug
//! is optional. The old numbered form (`/decklist/80000/…`) answers with a
//! 301 to the uuid one, so it is refused with that advice rather than
//! followed: following it would mean a request to the website, not the
//! API. A private deck shared by link (`/deck/view/…`) is not served by
//! the public API at all — `/api/v3/public/decks` answers 404 — so it is
//! refused with the reason. The API accepts the uuid in either case; the
//! reference keeps it lowercase so one deck has one address.
//!
//! **A search is the same list, filtered** (Phase 7 §9, 8 October 2026).
//! `GET /api/v3/public/decklists?filter[card_id]=<id>` answers with every
//! published decklist that plays the card, each row in the same shape as
//! a decklist fetched by its uuid — `card_slots` included, so a row picked
//! from the results imports with no second request. Read off the API on 8
//! October 2026: `filter[card_id]`, `filter[identity_card_id]`,
//! `filter[side_id]`, `filter[faction_id]` and `filter[user_id]` answer;
//! `sort=-created_at` puts the newest first; a page is twenty rows and
//! `links.next` pages on; `filter[search]` and `filter[format_id]` answer
//! 500 (as they did on 30 September), and `filter[name]` matches nothing
//! useful. So a search is by a card or an identity, newest first, one page:
//! the person names a card, the client resolves it to its id against the
//! catalog, and twenty newest lists is what a person reads before they
//! narrow the card.
//!
//! **Nothing here reads the network under test.** [`read_decklist`] and
//! [`read_decklists`] take the body, so the shape is pinned against a
//! canned answer, and [`fetch_decklist`] and [`fetch_decklists`] are the
//! two functions that go out.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::error::SyncError;
use crate::http::http_client;

/// Where the v3 API serves decklists.
const DECKLISTS_URL: &str = "https://api.netrunnerdb.com/api/v3/public/decklists";

/// A published decklist's identity: its uuid, lowercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecklistRef(String);

impl DecklistRef {
    /// Reads a decklist's uuid, or the link to its page on NetrunnerDB,
    /// as a person pastes it. What is refused says why, in words for the
    /// person: the old numbered link, a private deck's link, and anything
    /// that is neither a uuid nor a decklist link.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if is_uuid(text) {
            return Ok(DecklistRef(text.to_ascii_lowercase()));
        }
        let path = text.split_once("://").map_or(text, |(_, rest)| rest);
        let path = path.split(['?', '#']).next().unwrap_or("");
        let segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
        if let Some(at) = segments.iter().position(|segment| *segment == "decklist") {
            return match segments.get(at + 1) {
                Some(id) if is_uuid(id) => Ok(DecklistRef(id.to_ascii_lowercase())),
                Some(id) if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) => {
                    Err("That is NetrunnerDB's old numbered link. Open it in a browser and paste the address it lands on, which names the deck by its uuid".to_string())
                }
                _ => Err("That link has no decklist uuid after /decklist/".to_string()),
            };
        }
        if segments.windows(2).any(|pair| pair == ["deck", "view"]) {
            return Err("That is a private deck, which NetrunnerDB does not serve through its public API. Publish it as a decklist and paste that link".to_string());
        }
        Err("Not a NetrunnerDB decklist: paste a decklist's link (netrunnerdb.com/en/decklist/…) or its uuid".to_string())
    }

    pub fn uuid(&self) -> &str {
        &self.0
    }

    /// The API address the decklist is fetched from.
    pub fn api_url(&self) -> String {
        format!("{DECKLISTS_URL}/{}", self.0)
    }

    /// The decklist's page, for the person: the link kept with the deck.
    pub fn page_url(&self) -> String {
        format!("https://netrunnerdb.com/en/decklist/{}", self.0)
    }
}

/// `8-4-4-4-12` hex digits, either case.
fn is_uuid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5
        && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, len)| group.len() == len && group.chars().all(|c| c.is_ascii_hexdigit()))
}

/// A published decklist as NetrunnerDB gives it, before any catalog has
/// looked at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decklist {
    pub reference: DecklistRef,
    pub name: String,
    /// The author's NetrunnerDB user name; empty when the API gave none.
    pub author: String,
    /// The notes as served: HTML.
    pub notes: String,
    /// The identity's v3 card id.
    pub identity: String,
    /// Every card's v3 id with its copies, by id — the identity among
    /// them, as `card_slots` lists it.
    pub cards: Vec<(String, u32)>,
    /// When it was published, as the API gives it (RFC 3339); empty when
    /// it gave none. A search lists its rows by it.
    pub created_at: String,
}

/// What a search asks for: the lists that play one card, or are built
/// on one identity. One or the other — an identity is a card too, but
/// the API files it under its own filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Search {
    /// Lists that play the card with this v3 id.
    Card(String),
    /// Lists built on the identity with this v3 id.
    Identity(String),
}

impl Search {
    /// The request: the filter, newest first.
    pub fn api_url(&self) -> String {
        let filter = match self {
            Search::Card(id) => format!("filter[card_id]={id}"),
            Search::Identity(id) => format!("filter[identity_card_id]={id}"),
        };
        format!("{DECKLISTS_URL}?{filter}&sort=-created_at")
    }
}

#[derive(Deserialize)]
struct Envelope {
    #[serde(default)]
    data: Option<Resource>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

/// A search's answer: the same resources, as a list.
#[derive(Deserialize)]
struct ListEnvelope {
    #[serde(default)]
    data: Vec<Resource>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Deserialize)]
struct Resource {
    id: String,
    attributes: Attributes,
}

#[derive(Deserialize)]
struct Attributes {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    identity_card_id: Option<String>,
    #[serde(default)]
    card_slots: BTreeMap<String, u32>,
    #[serde(default)]
    created_at: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

/// Reads the API's answer for one decklist. An `errors` body is the
/// API's own refusal, worded as it gave it.
pub fn read_decklist(body: &str) -> Result<Decklist, SyncError> {
    let envelope: Envelope = serde_json::from_str(body)?;
    refused(&envelope.errors)?;
    let Some(resource) = envelope.data else { return Err(SyncError::DecklistShape("the answer holds no decklist".to_string())) };
    decklist_of(resource)
}

/// Reads the API's answer to a search: the rows it holds, in its order
/// (newest first, as asked). A row the client cannot read — no identity,
/// an id that is not a uuid — is left out rather than failing the page.
pub fn read_decklists(body: &str) -> Result<Vec<Decklist>, SyncError> {
    let envelope: ListEnvelope = serde_json::from_str(body)?;
    refused(&envelope.errors)?;
    Ok(envelope.data.into_iter().filter_map(|resource| decklist_of(resource).ok()).collect())
}

fn refused(errors: &[ApiError]) -> Result<(), SyncError> {
    if let Some(error) = errors.first() {
        let reason = error.title.clone().or_else(|| error.code.clone()).unwrap_or_else(|| "an error with no reason".to_string());
        return Err(SyncError::DecklistRefused(reason));
    }
    Ok(())
}

fn decklist_of(resource: Resource) -> Result<Decklist, SyncError> {
    let reference = DecklistRef::parse(&resource.id).map_err(|_| SyncError::DecklistShape(format!("the decklist's id {:?} is not a uuid", resource.id)))?;
    let attributes = resource.attributes;
    let Some(identity) = attributes.identity_card_id.filter(|id| !id.is_empty()) else {
        return Err(SyncError::DecklistShape("the decklist names no identity".to_string()));
    };
    Ok(Decklist {
        reference,
        name: attributes.name.unwrap_or_default(),
        author: attributes.user_id.unwrap_or_default(),
        notes: attributes.notes.unwrap_or_default(),
        identity,
        cards: attributes.card_slots.into_iter().collect(),
        created_at: attributes.created_at.unwrap_or_default(),
    })
}

/// Fetches the decklist `reference` names. A 404 is "no such published
/// decklist" in the person's words; any other failure is the request's.
pub async fn fetch_decklist(reference: &DecklistRef) -> Result<Decklist, SyncError> {
    let response = http_client().get(reference.api_url()).send().await?;
    let status = response.status();
    if status.as_u16() == 404 {
        return Err(SyncError::DecklistNotFound { id: reference.uuid().to_string() });
    }
    if !status.is_success() {
        return Err(SyncError::DecklistDownload { id: reference.uuid().to_string(), status: status.as_u16() });
    }
    read_decklist(&response.text().await?)
}

/// Fetches the first page of the lists `search` asks for, newest first.
pub async fn fetch_decklists(search: &Search) -> Result<Vec<Decklist>, SyncError> {
    let response = http_client().get(search.api_url()).send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(SyncError::SearchDownload { status: status.as_u16() });
    }
    read_decklists(&response.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "99ba7131-6cf1-474e-b73f-8b1aefc93d56";

    /// Trimmed from the live answer for this decklist on 30 September
    /// 2026; the fields the client reads are as served.
    const BODY: &str = r#"{"data":{"id":"99ba7131-6cf1-474e-b73f-8b1aefc93d56","type":"decklists","attributes":{"user_id":"WonkyWombat","follows_basic_deckbuilding_rules":true,"identity_card_id":"hoshiko_shiro_untold_protagonist","name":"1st at Oops! all IDs: Dreamnet hosh","notes":"<p>played this in a tournament.</p>\n","tags":null,"side_id":"runner","created_at":"2026-09-26T15:00:15+00:00","updated_at":"2026-09-26T15:03:05+00:00","faction_id":"anarch","card_slots":{"botulus":1,"hoshiko_shiro_untold_protagonist":1,"sure_gamble":3},"num_cards":45,"influence_spent":15},"relationships":{},"links":{"self":"https://api.netrunnerdb.com/api/v3/public/decklists/99ba7131-6cf1-474e-b73f-8b1aefc93d56"}},"meta":{}}"#;

    /// Trimmed from the live answer to `filter[user_id]=WonkyWombat` on 8
    /// October 2026: two rows, each in the shape a single decklist has.
    const LIST_BODY: &str = r#"{"data": [{"id": "bb1a4430-ffab-419c-a044-4a2e8ff79830", "type": "decklists", "attributes": {"user_id": "WonkyWombat", "follows_basic_deckbuilding_rules": true, "identity_card_id": "cerebral_imaging_infinite_frontiers", "name": "1st at Oops! all IDs: core ci", "notes": "<p>x</p>", "tags": null, "side_id": "corp", "created_at": "2026-09-26T14:55:39+00:00", "updated_at": "2026-09-26T15:03:05+00:00", "faction_id": "haas_bioroid", "card_slots": {"big_deal": 3, "bran_1_0": 3, "cerebral_imaging_infinite_frontiers": 1}, "num_cards": 49, "influence_spent": 15}}, {"id": "99ba7131-6cf1-474e-b73f-8b1aefc93d56", "type": "decklists", "attributes": {"user_id": "WonkyWombat", "follows_basic_deckbuilding_rules": true, "identity_card_id": "hoshiko_shiro_untold_protagonist", "name": "1st at Oops! all IDs: Dreamnet hosh", "notes": "<p>x</p>", "tags": null, "side_id": "runner", "created_at": "2026-09-26T15:00:15+00:00", "updated_at": "2026-09-26T15:03:05+00:00", "faction_id": "anarch", "card_slots": {"botulus": 1, "bravado": 1, "clean_getaway": 2}, "num_cards": 45, "influence_spent": 15}}], "meta": {"stats": {"total": {"count": 15}}}, "links": {"self": "https://api.netrunnerdb.com/api/v3/public/decklists?filter%5Buser_id%5D=WonkyWombat&page%5Bnumber%5D=1&page%5Bsize%5D=20&stats%5Btotal%5D=count"}}"#;

    #[test]
    fn a_search_asks_by_card_or_identity_newest_first_and_reads_a_page_of_rows() {
        assert_eq!(Search::Card("sure_gamble".into()).api_url(), format!("{DECKLISTS_URL}?filter[card_id]=sure_gamble&sort=-created_at"));
        assert_eq!(Search::Identity("hoshiko_shiro_untold_protagonist".into()).api_url(), format!("{DECKLISTS_URL}?filter[identity_card_id]=hoshiko_shiro_untold_protagonist&sort=-created_at"));
        let rows = read_decklists(LIST_BODY).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "1st at Oops! all IDs: core ci");
        assert_eq!(rows[0].identity, "cerebral_imaging_infinite_frontiers");
        assert_eq!(rows[0].created_at, "2026-09-26T14:55:39+00:00");
        assert_eq!(rows[1].reference.uuid(), UUID);
        assert_eq!(rows[1].cards, vec![("botulus".to_string(), 1), ("bravado".to_string(), 1), ("clean_getaway".to_string(), 2)], "a row carries its cards, so a pick imports with no second request");
        let refused = read_decklists(r#"{"errors":[{"code":"internal_server_error","status":"500","title":"Internal Server Error"}]}"#).unwrap_err();
        assert!(matches!(refused, SyncError::DecklistRefused(reason) if reason == "Internal Server Error"));
        assert!(read_decklists(r#"{"data":[]}"#).unwrap().is_empty(), "no rows is no lists, not a failure");
    }

    #[test]
    fn a_reference_is_a_uuid_or_a_decklist_link_in_any_of_its_forms() {
        let expected = DecklistRef(UUID.to_string());
        assert_eq!(DecklistRef::parse(UUID).unwrap(), expected);
        assert_eq!(DecklistRef::parse(&format!("  {}\n", UUID.to_ascii_uppercase())).unwrap(), expected, "either case, trimmed");
        assert_eq!(DecklistRef::parse(&format!("https://netrunnerdb.com/en/decklist/{UUID}/1st-at-oops-all-ids-dreamnet-hosh")).unwrap(), expected);
        assert_eq!(DecklistRef::parse(&format!("https://netrunnerdb.com/fr/decklist/{UUID}")).unwrap(), expected, "any language, no slug");
        assert_eq!(DecklistRef::parse(&format!("netrunnerdb.com/en/decklist/{UUID}/slug?x=1#top")).unwrap(), expected, "no scheme, a query and a fragment");
        assert_eq!(expected.api_url(), format!("https://api.netrunnerdb.com/api/v3/public/decklists/{UUID}"));
        assert_eq!(expected.page_url(), format!("https://netrunnerdb.com/en/decklist/{UUID}"));
    }

    #[test]
    fn what_is_refused_says_why() {
        let numbered = DecklistRef::parse("https://netrunnerdb.com/en/decklist/80000/some-deck").unwrap_err();
        assert!(numbered.contains("old numbered link"), "{numbered}");
        let private = DecklistRef::parse(&format!("https://netrunnerdb.com/en/deck/view/{UUID}")).unwrap_err();
        assert!(private.contains("private deck"), "{private}");
        let words = DecklistRef::parse("just words").unwrap_err();
        assert!(words.starts_with("Not a NetrunnerDB decklist"), "{words}");
        assert!(DecklistRef::parse("").is_err());
        assert!(DecklistRef::parse("https://netrunnerdb.com/en/decklist/").is_err(), "nothing after /decklist/");
        assert!(DecklistRef::parse("99ba7131-6cf1-474e-b73f-8b1aefc93d5").is_err(), "a digit short");
    }

    #[test]
    fn a_decklist_is_read_off_the_v3_answer_with_the_identity_among_its_cards() {
        let list = read_decklist(BODY).unwrap();
        assert_eq!(list.reference.uuid(), UUID);
        assert_eq!(list.name, "1st at Oops! all IDs: Dreamnet hosh");
        assert_eq!(list.author, "WonkyWombat");
        assert_eq!(list.notes, "<p>played this in a tournament.</p>\n");
        assert_eq!(list.identity, "hoshiko_shiro_untold_protagonist");
        assert_eq!(list.cards, vec![("botulus".to_string(), 1), ("hoshiko_shiro_untold_protagonist".to_string(), 1), ("sure_gamble".to_string(), 3)]);
    }

    /// The one request that goes out, run by hand (`cargo test -p
    /// netrunner_card_sync -- --ignored`): the live API still answers in
    /// the shape [`read_decklist`] reads, and a uuid nobody published is
    /// the not-found error. Ignored so CI never reaches the network.
    #[test]
    #[ignore = "asks the live NetrunnerDB API"]
    fn the_live_api_answers_in_the_shape_read_here() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let list = runtime.block_on(fetch_decklist(&DecklistRef::parse(UUID).unwrap())).unwrap();
        assert_eq!(list.identity, "hoshiko_shiro_untold_protagonist");
        assert!(list.cards.iter().any(|(id, count)| id == "sure_gamble" && *count == 3), "{:?}", list.cards);
        let missing = runtime.block_on(fetch_decklist(&DecklistRef::parse("00000000-0000-0000-0000-000000000000").unwrap())).unwrap_err();
        assert!(matches!(missing, SyncError::DecklistNotFound { .. }), "{missing}");
    }

    #[test]
    fn the_apis_refusal_and_a_wrong_shape_are_errors_in_words() {
        let refused = read_decklist(r#"{"errors":[{"code":"not_found","status":"404","title":"Not Found"}]}"#).unwrap_err();
        assert_eq!(refused.to_string(), "NetrunnerDB refused the decklist: Not Found");
        let empty = read_decklist(r#"{"meta":{}}"#).unwrap_err();
        assert!(empty.to_string().contains("holds no decklist"), "{empty}");
        let no_identity = read_decklist(r#"{"data":{"id":"99ba7131-6cf1-474e-b73f-8b1aefc93d56","attributes":{"name":"x"}}}"#).unwrap_err();
        assert!(no_identity.to_string().contains("names no identity"), "{no_identity}");
        assert!(read_decklist("not json").is_err());
    }
}
