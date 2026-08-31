//! `netrunner_cli deck ...` — listing, inspecting, building and editing
//! saved decks.
//!
//! Deck *rules* live in `netrunner_core` and file handling in
//! `deck_store`; this module is presentation and the edit commands' small
//! amount of glue. It is deliberately not `async` — unlike `cards::run`
//! there is no network here, and a synchronous command is simpler to read.

use netrunner_core::cards::{register_playable_cards, CardRegistry};
use netrunner_core::decks::{DeckCategory, DeckEntry, DeckFile};
use netrunner_core::dsl::{CardDefinition, CardId, CardType};
use netrunner_core::format::NsgFormat;
use netrunner_core::rules::Side;

use crate::config::{Config, DeckAction};
use crate::deck_store::{self, Origin, StoredDeck};

pub fn run(action: DeckAction, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let dir = deck_store::resolve_decks_dir(config.decks_dir.as_deref())?;
    let format: NsgFormat = config.format.into();
    let mut registry = CardRegistry::new();
    register_playable_cards(&mut registry);

    match action {
        DeckAction::List => {
            let decks = deck_store::list(&dir)?;
            println!("Deck directory: {}\n", dir.display());
            for stored in &decks {
                let origin = if stored.origin.is_embedded() { "built-in" } else { "saved" };
                println!(
                    "{:<22} {:<6} {:<8} {:<8} {:>2} cards  {}",
                    stored.deck.id,
                    side_label(stored.deck.side),
                    category_label(stored.deck.category),
                    origin,
                    stored.deck.size(),
                    stored.deck.name
                );
                if let Some(description) = &stored.deck.description {
                    println!("{:<22} {description}", "");
                }
            }
            if decks.is_empty() {
                println!("(no decks)");
            }
        }

        DeckAction::Show { name } => {
            let stored = deck_store::load(&dir, &name)?;
            show(&stored, &registry, format);
        }

        DeckAction::Validate { name } => {
            let stored = deck_store::load(&dir, &name)?;
            match stored.deck.validate(&registry, format) {
                Ok(report) => println!("{} is legal.\n{}", stored.deck.id, describe_report(&report)),
                // Returned as an error, not just printed, so `deck validate`
                // is usable as a gate in a script.
                Err(e) => return Err(format!("{} is not legal: {e}", stored.deck.id).into()),
            }
        }

        DeckAction::New { name, side, identity } => {
            let side: Side = side.into();
            let identity = find_card(&registry, &identity)?;
            if identity.card_type != CardType::Identity {
                return Err(format!("{:?} is not an identity card", identity.title).into());
            }
            if identity.side != side {
                return Err(
                    format!("{:?} is a {:?} identity, not {side:?}", identity.title, identity.side).into()
                );
            }

            let deck = DeckFile {
                id: name.clone(),
                name,
                side,
                category: DeckCategory::Custom,
                description: None,
                how_to_play: None,
                identity: identity.id.clone(),
                cards: Vec::new(),
            };
            let path = deck_store::save(&dir, &deck)?;
            println!("Created {} ({:?}).", path.display(), identity.title);
            println!("Add cards with: netrunner_cli deck add {} <card> [count]", deck.id);
        }

        DeckAction::Add { name, card, count } => {
            let (mut deck, _) = load_editable(&dir, &name)?;
            let definition = find_card(&registry, &card)?;

            match deck.cards.iter_mut().find(|entry| entry.card == definition.id) {
                Some(entry) => entry.count += count,
                None => deck.cards.push(DeckEntry { card: definition.id.clone(), count }),
            }

            let total = copies_of(&deck, &definition.id);
            deck_store::save(&dir, &deck)?;
            println!("Added {count} x {:?} to {} ({total} total, {} cards).", definition.title, deck.id, deck.size());
            report_progress(&deck, &registry, format);
        }

        DeckAction::Remove { name, card, count } => {
            let (mut deck, _) = load_editable(&dir, &name)?;
            let definition = find_card(&registry, &card)?;

            let entry = deck
                .cards
                .iter_mut()
                .find(|entry| entry.card == definition.id)
                .ok_or_else(|| format!("{} does not contain {:?}", deck.id, definition.title))?;
            entry.count = entry.count.saturating_sub(count);
            deck.cards.retain(|entry| entry.count > 0);

            let total = copies_of(&deck, &definition.id);
            deck_store::save(&dir, &deck)?;
            println!("Removed {count} x {:?} from {} ({total} left, {} cards).", definition.title, deck.id, deck.size());
            report_progress(&deck, &registry, format);
        }
    }

    Ok(())
}

/// Loads a deck that is about to be edited, refusing the built-in ones.
///
/// Caught here as well as in `deck_store::save` so the refusal arrives
/// before any work is done, and names editing rather than saving as the
/// thing that is not allowed.
fn load_editable(dir: &std::path::Path, name: &str) -> Result<(DeckFile, StoredDeck), String> {
    let stored = deck_store::load(dir, name)?;
    if let Origin::Embedded = stored.origin {
        return Err(format!(
            "{name:?} is a built-in deck and cannot be edited; \
             copy it to your deck directory under a new id first"
        ));
    }
    Ok((stored.deck.clone(), stored))
}

fn copies_of(deck: &DeckFile, card: &CardId) -> u32 {
    deck.cards.iter().filter(|entry| &entry.card == card).map(|entry| entry.count).sum()
}

/// Reports legality after an edit **as a note, never as a failure**.
///
/// A deck under construction is legitimately illegal — too few cards, agenda
/// points not yet in range — so refusing the edit would make it impossible to
/// build a deck one card at a time. `deck validate` and starting a match are
/// the gates; `deck add` is not.
fn report_progress(deck: &DeckFile, registry: &CardRegistry, format: NsgFormat) {
    match deck.validate(registry, format) {
        Ok(report) => println!("Legal. {}", describe_report(&report)),
        Err(e) => println!("Not legal yet: {e}"),
    }
}

fn show(stored: &StoredDeck, registry: &CardRegistry, format: NsgFormat) {
    let deck = &stored.deck;
    println!("{}  ({})", bold(&deck.name), deck.id);
    println!(
        "{} · {} · {}",
        side_label(deck.side),
        category_label(deck.category),
        match &stored.origin {
            Origin::Embedded => "built-in".to_string(),
            Origin::Disk(path) => path.display().to_string(),
        }
    );

    let identity = registry.get(&deck.identity);
    println!("Identity: {}", identity.map_or(deck.identity.0.clone(), |card| card.title.clone()));

    if let Some(description) = &deck.description {
        println!("\n{description}");
    }

    println!("\n{}", bold(&format!("Cards ({})", deck.size())));
    let mut entries: Vec<(&DeckEntry, Option<&CardDefinition>)> =
        deck.cards.iter().map(|entry| (entry, registry.get(&entry.card))).collect();
    // Grouped by printed type, then by title, so a deck reads the way a
    // decklist is normally written rather than in file order.
    entries.sort_by_key(|(entry, card)| {
        (type_label(*card), card.map_or(entry.card.0.clone(), |card| card.title.clone()))
    });

    let mut current_type = "";
    for (entry, card) in entries {
        let label = type_label(card);
        if current_type != label {
            println!("  {label}");
            current_type = label;
        }
        let title = card.map_or(entry.card.0.clone(), |card| card.title.clone());
        println!("    {}x {title}", entry.count);
    }

    if let Some(how_to_play) = &deck.how_to_play {
        println!("\n{}", bold("How to play"));
        print!("{}", render_markdown(how_to_play));
    }

    println!();
    match deck.validate(registry, format) {
        Ok(report) => println!("{}", describe_report(&report)),
        Err(e) => println!("Not legal in {format:?}: {e}"),
    }
}

fn describe_report(report: &netrunner_core::deck::ValidationReport) -> String {
    format!(
        "{:?}-legal · {} cards · {:?} · {} influence spent{}",
        report.format,
        report.deck_size,
        report.identity_faction,
        report.influence_spent,
        report.agenda_points.map_or(String::new(), |points| format!(" · {points} agenda points"))
    )
}

/// Finds a card by registry id, falling back to an exact title match.
///
/// Id first because it is unambiguous and is what deck files store; titles
/// are the fallback because nobody wants to type `rene_loup_arcemont`. An
/// ambiguous title lists the candidates' ids rather than picking one — the
/// same "show what is actually available" convention the deck-name errors
/// use.
fn find_card<'r>(registry: &'r CardRegistry, needle: &str) -> Result<&'r CardDefinition, String> {
    if let Some(card) = registry.get(&CardId(needle.to_string())) {
        return Ok(card);
    }

    let matches: Vec<&CardDefinition> = registry.iter().filter(|card| card.title == needle).collect();
    match matches.as_slice() {
        [card] => Ok(card),
        [] => Err(format!("no card with id or title {needle:?}")),
        many => Err(format!(
            "{needle:?} matches {} cards; use one of these ids: {}",
            many.len(),
            many.iter().map(|card| card.id.0.as_str()).collect::<Vec<_>>().join(", ")
        )),
    }
}

/// Renders the small slice of Markdown a how-to-play note actually uses:
/// ATX headings and list items. Everything else passes through unchanged.
///
/// Hand-rolled rather than pulling in a Markdown crate — the prose is meant
/// to stay readable as plain text, so there is very little to do, and a
/// parser would be a dependency earning almost nothing.
fn render_markdown(source: &str) -> String {
    let mut out = String::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(heading) = trimmed.strip_prefix("###").or_else(|| trimmed.strip_prefix("##")).or_else(|| trimmed.strip_prefix('#')) {
            out.push_str(&format!("  {}\n", bold(heading.trim())));
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            out.push_str(&format!("    • {item}\n"));
        } else if trimmed.is_empty() {
            out.push('\n');
        } else {
            out.push_str(&format!("  {trimmed}\n"));
        }
    }
    out
}

/// Bold via `crossterm`, which is already a dependency for the TUI. Falls
/// back to plain text when stdout is not a terminal, so piping `deck show`
/// into a file does not litter it with escape codes.
fn bold(text: &str) -> String {
    use crossterm::style::Stylize;
    if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        text.to_string().bold().to_string()
    } else {
        text.to_string()
    }
}

/// The printed card type, as a player would say it.
///
/// `CardType::Ice` carries its subtype in the enum, but a decklist groups by
/// the printed type — a Barrier and a Sentry are both ice — so the payload
/// is dropped here rather than splitting the listing into a section per
/// subtype. Written out instead of `Debug`, which would render `Ice(Barrier)`.
fn type_label(card: Option<&CardDefinition>) -> &'static str {
    match card.map(|card| &card.card_type) {
        Some(CardType::Agenda) => "Agenda",
        Some(CardType::Asset) => "Asset",
        Some(CardType::Operation) => "Operation",
        Some(CardType::Ice(_)) => "Ice",
        Some(CardType::Upgrade) => "Upgrade",
        Some(CardType::Identity) => "Identity",
        Some(CardType::Event) => "Event",
        Some(CardType::Hardware) => "Hardware",
        Some(CardType::Program) => "Program",
        Some(CardType::Resource) => "Resource",
        None => "Unknown",
    }
}

fn side_label(side: Side) -> &'static str {
    match side {
        Side::Corp => "Corp",
        Side::Runner => "Runner",
    }
}

fn category_label(category: DeckCategory) -> &'static str {
    match category {
        DeckCategory::Sample => "sample",
        DeckCategory::Starter => "starter",
        DeckCategory::Boosted => "boosted",
        DeckCategory::Custom => "custom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        register_playable_cards(&mut registry);
        registry
    }

    #[test]
    fn a_card_resolves_by_id_or_by_title() {
        let registry = registry();
        assert_eq!(find_card(&registry, "hedge_fund").expect("by id").title, "Hedge Fund");
        assert_eq!(find_card(&registry, "Hedge Fund").expect("by title").id.0, "hedge_fund");
    }

    #[test]
    fn an_unknown_card_is_reported_with_what_was_asked_for() {
        let registry = registry();
        let err = find_card(&registry, "Not A Card").expect_err("unknown card");
        assert!(err.contains("Not A Card"), "{err}");
    }

    #[test]
    fn markdown_headings_and_lists_are_rendered() {
        let rendered = render_markdown("## Opening\n\n- Mulligan for Cleaver.\nPlain line.");
        assert!(rendered.contains("Opening"));
        assert!(rendered.contains("• Mulligan for Cleaver."));
        assert!(rendered.contains("Plain line."));
        assert!(!rendered.contains('#'), "heading markers should not survive: {rendered}");
    }
}
