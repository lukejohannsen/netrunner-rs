//! The ledger of what the clients do with every field of the view: each
//! struct a seat receives is destructured here with **every field named**,
//! so a field added to `ClientView`, or to a struct inside it, does not
//! compile under `cargo test` or `cargo clippy --all-targets` until
//! someone writes down what the clients do with it.
//!
//! **Why it exists.** Each Vantage Point stage put in the view what its
//! cards needed, and nothing asked whether a person could see it: a flip
//! identity's side, Méliès U's copy, the Runner's cards out of the game,
//! each scored card's worth, a credit pool locked for a run — all carried,
//! none drawn, until a read of the whole view after the set (#250). The
//! person asked (27 September 2026) that the UI work a card update makes be
//! tracked and shipped with the update as it goes, and this is where it is
//! caught: the stage that adds the field writes its line here.
//!
//! Every field is one of three things, in its comment:
//!
//! - **drawn** — where both clients show it (a `netrunner_client` module
//!   both stand on, or each client's own place).
//! - **engine's** — carried for the engine's own reasons (a bot's
//!   determinized sample, legality, bookkeeping), with why a person needs
//!   no picture of it.
//! - **OWED** — a person should see it and does not yet. Each is a row of
//!   the client ledger in `docs/roadmap/nsg-card-pool.md`, which is where
//!   the debt is tracked; the row leaves when the field is drawn, and this
//!   comment changes with it.
//!
//! The enums a client words are already exhaustive where they are worded
//! — `GameEvent` in `actions` (the log), `PlayerAction` in `actions` and
//! `board::affordance`, `lingering::Lingering` in `board::hud::in_effect`
//! — so a new variant is caught there. `PendingDecision`'s variants are
//! matched by `board::Prompt` with `..`, so their fields are listed here.

#![cfg(test)]

use netrunner_core::rules::{
    PendingDecision, PublicAccessState, PublicArchivedCard, PublicInstalledCard, PublicInstalledRunnerCard, PublicPendingPayment, PublicRunIce,
    PublicRunIceIdentity, PublicRunState, ScoredAgenda,
};
use netrunner_core::view::{ClientView, CorpClientView, RunnerClientView, ServerView};

#[allow(dead_code, clippy::too_many_lines)]
fn every_field_of_the_view_is_accounted_for(view: ClientView) {
    let ClientView {
        viewer: _,             // engine's: whose eyes; every client module reads it to decide what to offer
        active_player: _,      // drawn: desktop avatar bar lit for the side whose turn it is, board::timing
        phase: _,              // drawn: board::phase (desktop phase panel), the terminal's header
        turn: _,               // drawn: board::phase, the terminal's header
        next_install_id: _,    // engine's: the id the next install gets, so a bot's sample installs under the real one; a person never names a card by it
        this_turn: _,          // engine's: the turn log's counts and sums (Pichação's clicks gained during runs among them), read by cards and bots; the log shows each event
        last_turn: _,          // engine's: as this_turn, for the turn before
        rules: _,              // drawn: the points to win, on the HUD's Agendas readout (hud::readouts)
        corp,                  // below
        runner,                // below
        active_run,            // below
        paid_ability_window: _, // drawn: Prompt::of ("Paid ability window: … has priority"), board::phase
        active_trace: _,       // drawn: Prompt::of (base strength, the Corp's bid)
        pending_prevention: _, // drawn: Prompt::of and Prompt::card (what would happen, the card offering)
        pending_paid_choice: _, // drawn: Prompt::of and Prompt::card
        pending_decision,      // below
        pending_payment,       // below
        lingering: _,          // drawn: hud::in_effect (desktop rail, terminal under the servers); a strength is on its card
        delayed: _,            // drawn: hud::in_effect ("when this turn ends, …"), both clients
        standing_cannot: _,    // drawn: hud::in_effect ("Attini: the Runner cannot spend credits"), both clients
        revealed: _,           // drawn: hud::in_effect ("revealed in HQ: …"), both clients; the chooser's pop-up draws them as its cards
        selection: _,          // drawn: selection::Selection (the pop-up's cards, the terminal's card in question)
        legal_actions: _,      // drawn: board::ActionMap — every action is a control, a menu entry or a pop-up button
    } = view;

    let CorpClientView {
        credits: _,             // drawn: hud::readouts
        clicks: _,              // drawn: hud::readouts
        agenda_points: _,       // drawn: hud::readouts
        bad_publicity: _,       // drawn: hud::readouts
        recurring_credits: _,   // drawn: hud::identity_facts / identity_chip ("2 of 2 recurring credits left")
        recurring_credits_max: _, // drawn: with recurring_credits
        hq_count: _,            // drawn: HQ's column header, the terminal's server line
        hq_cards: _,            // drawn: the Corp's own hand in both clients
        rd_count: _,            // drawn: R&D's column header, the terminal's server line
        archives,               // below
        servers,                // below
        scored_agendas,         // below
        scored_worth: _,        // drawn: hud::score_area, each row's points
        points_to_win: _,       // drawn: hud::readouts ("3/5"), the score sheet's caption, the terminal's status line
        removed_from_game: _,   // drawn: under Archives' sheet; the terminal's identity line and card picker
        identity_counters: _,   // drawn: hud::identity_facts / identity_chip (AU Co.'s power counters)
        identity_flipped: _,    // drawn: hud::identity_side — the avatar chip, the identity sheet, the terminal's identity line
        identity_copy: _,       // drawn: hud::identity_side ("Side 2")
        once_per_turn_used: _,  // engine's: a use limit the action list already honours; board::rig tells copies apart by it
        identity: _,            // drawn: the avatar and its sheet, the terminal's identity line
    } = corp;

    let RunnerClientView {
        credits: _,             // drawn: hud::readouts, with a run's own credits beside it
        clicks: _,              // drawn: hud::readouts
        agenda_points: _,       // drawn: hud::readouts
        memory_units: _,        // drawn: hud::details
        tags: _,                // drawn: hud::readouts
        brain_damage: _,        // drawn: hud::readouts ("Core damage")
        grip_count: _,          // drawn: hud::readouts ("Grip")
        grip_cards: _,          // drawn: the Runner's own hand in both clients
        stack_count: _,         // drawn: the Stack pile button, the terminal's pile line
        heap: _,                // drawn: the Heap pile button and sheet, the terminal's pile line
        removed_from_game: _,   // drawn: under the Heap's sheet; the terminal's identity line and card picker
        set_aside: _,           // drawn: hud::in_effect ("set aside: …"), both clients; the chooser's pop-up draws the ones it offers
        rig,                    // below
        link_strength: _,       // drawn: hud::details
        scored_agendas: runner_scored, // below, as the Corp's
        scored_worth: _,        // drawn: hud::score_area, each row's points
        points_to_win: _,       // drawn: as the Corp's
        servers_run_this_turn: _, // engine's: Red Team's legality and the evaluator; each run is in the log
        discarded_this_discard_phase: _, // engine's: re-evaluating a parked Magdalene choice in a sample; the discards are in the heap
        identity_flipped: _,    // drawn: hud::identity_side
        once_per_turn_used: _,  // engine's: as the Corp's
        identity: _,            // drawn: the avatar and its sheet, the terminal's identity line
    } = runner;

    for ServerView {
        server: _, // drawn: a column and its plate; a line of the terminal's
        ice,       // below
        root,      // below
    } in servers
    {
        for PublicInstalledCard {
            install_id: _,         // engine's: the handle actions and the board's clicks name
            position: _,           // drawn: board::facts ("outermost of 3, met first")
            server: _,             // drawn: the column it is in
            slot: _,               // drawn: ice or root, board::facts
            rezzed: _,             // drawn: board::facts tile_title and the tile's face; "faceup" for an agenda
            card: _,               // drawn: the tile's face, or its back when hidden
            advancement_tokens: _, // drawn: board::facts tile_tokens
            counters: _,           // drawn: board::facts tile_tokens
            advancement_requirement: _, // drawn: board::facts tile_tokens ("2/3") and install_facts ("Prints … its text makes it …"); the terminal's server line ("2/3 adv")
            seen_by_runner: _,     // drawn: facts::seen_face_down — "seen" on the Corp's tile, a line on its sheet, the terminal's server line
        } in ice.into_iter().chain(root)
        {}
    }

    for ScoredAgenda {
        card: _,                     // drawn: hud::score_area
        install_id: _,               // drawn: a scored agenda's abilities, on its row (hud::ScoredCard::install)
        agenda_counters: _,          // drawn: hud::ScoredCard::facts
        scored_on_turn: _,           // engine's: "scored this turn" filters; the score is in the log
        installed_on_scoring_turn: _, // engine's: "installed this turn" filters on a score-area copy
        advanced_on_scoring_turn: _,  // engine's: "advanced this turn" filters on a score-area copy (Issuaq Adaptics)
        as_agenda: _,                // drawn: hud::ScoredCard::facts ("Added as an assassination agenda", "Cannot be forfeited")
    } in scored_agendas.into_iter().chain(runner_scored)
    {}

    for PublicInstalledRunnerCard {
        card: _,                  // drawn: the rig's rows in both clients
        install_id: _,            // engine's: the handle
        current_strength: _,      // drawn: the rig's chip line, board::facts
        hosted_on_ice: _,         // drawn: a Trojan's chip on its ice and its ghost (board::rig)
        hosted_on_rig_card: _,    // drawn: board::facts ("Hosted on …"), board::rig
        hosted_cards: _,          // drawn: board::facts ("Hosts …")
        hosted_facedown: _,       // drawn: board::facts ("Hosts facedown …")
        hosted_unseen: _,         // drawn: board::facts ("Hosts 2 cards facedown")
        turned_facedown: _,       // drawn: board::rig::hosted_chip ("3 hosted, 1 facedown") on both clients' rig lines; board::facts
        hosted_cards_playable: _, // engine's: the action list offers the hosted cards; drawn as they are hosted
        counters: _,              // drawn: the rig's chip line, board::facts
    } in rig
    {}

    if let Some(PublicRunState {
        server: _,               // drawn: the attacked column, board::trail, Prompt::of
        phase: _,                // drawn: board::phase, board::timing, the terminal's run strip
        ice,                     // below
        position: _,             // drawn: board::trail, the encountered ice's panel
        access_state,            // below
        jack_out_permitted: _,   // drawn: the Jack out control, greyed when it is not
        declared_successful: _,  // drawn: board::onward's words for the run's next step
        breach_only: _,          // drawn: board::phase ("Breach of R&D", its access the one step, the desktop's panel) and the terminal's run strip
        event_counters: _,       // drawn: hud::in_effect ("This run: Spree has 2 power counters"), both clients
        gained_for_the_run: _,   // drawn: the encounter's list (facts::install_facts), and outside it the ice's sheet ("» … — for the rest of this run") and hud::in_effect, both clients
        forced_encounter: _,     // drawn: board::phase ("Encounter ice 1 of 2 again"), the terminal's run strip ("encountering it again")
        bad_publicity_credits: _, // drawn: hud::readouts, beside the Runner's credits
        bonus_run_credits: _,    // drawn: hud::readouts, beside the Runner's credits
        run_credits_pay_for: _,  // drawn: hud::in_effect ("This run: the 4 [credit] on Bahia Bands may be spent only to pay trash costs"), both clients
        redirect_on_approach: _, // drawn: hud::in_effect ("This run: … the attacked server becomes HQ instead")
        fully_broken: _,         // drawn: every subroutine's broken mark on the encountered ice says it (board::facts)
        ice_derezzed: _,         // drawn: hud::in_effect ("This run: a piece of ice has been derezzed"), both clients; Stegodon MK IV's −2 it turns on is in every icebreaker's strength
        subroutine_broken: _,    // engine's: each break is a line of the log and a mark on the ice, and Mercury: Chrome Libertador's bonus access, which it turns off, is an offer the person sees or does not
        reached_success_phase: _, // engine's: whether the run got to the server, which the run's trail and log already draw; Hannah "Wheels" Pilintra's tag it decides is a log line
        breached: _, // engine's: a breach of another server is the log's "the Runner breached R&D", and every access prompt names the card's server
        encounters: _, // engine's: a count S-Dobrado reads; each encounter is drawn by board::trail as it happens
        this_encounter: _,       // engine's: a limit the action list and board::breaks already honour, as once_per_run_used; and which kinds of breaker broke the printed subroutines (Virtual Service Agent), each break already a line of the log; and the once-per-encounter abilities used (Slap Vandal), a limit the action list honours; and how many subroutines were broken (Flux Capacitor's first time each encounter), each break a line of the log
        once_per_run_used: _,    // engine's: a use limit the action list and board::breaks already honour, as once_per_turn_used
        initiated_by: _,         // drawn: names the run's event on the ability it has for the run, actions::describe (both clients' buttons)
        begun_as_the_turn_began: _, // engine's: where the turn goes when the run ends, which board::phase draws when it gets there
    }) = active_run
    {
        for PublicRunIce {
            install_id: _, // engine's: the handle
            rezzed: _,     // drawn: board::trail
            identity,      // below
        } in ice
        {
            if let Some(PublicRunIceIdentity {
                card: _,             // drawn: the encountered ice's panel
                current_strength: _, // drawn: board::facts strength_words
                ice_type: _,         // drawn: the strip's colour (Theme::tile)
                subroutines: _,      // drawn: board::facts encounter_subroutines, gained ones first
            }) = identity
            {}
        }
        if let Some(PublicAccessState {
            server: _,          // drawn: access::Access
            candidates: _,      // drawn: the choice of card to access (selection)
            from_zone: _,       // drawn: access::Access
            resolved_cards: _,  // drawn: board::trail
            pending_install: _, // engine's: which install is being accessed; access::Access shows the card
            phase: _,           // drawn: access::Access and Prompt::of
        }) = access_state
        {}
    }

    if let Some(PublicPendingPayment {
        side: _, // drawn: Prompt::of ("The Runner is choosing how to pay")
        own: _,  // drawn: Prompt::of and the pop-up's buttons, to the payer
    }) = pending_payment
    {}

    if let Some(decision) = pending_decision {
        match decision {
            PendingDecision::ChooseEffect {
                chooser: _,        // drawn: Prompt::card, only to the one asked
                options: _,        // engine's: resolved by index; the choice's buttons are its texts
                option_texts: _,   // drawn: the buttons (the printed clause each option quotes)
                source_card: _,    // drawn: Prompt::of's title, Prompt::card
                prompting_card: _, // drawn: as source_card
                source_install: _, // engine's: the handle the effect resolves on
                resume: _,         // engine's: what happens after
            } => {}
            PendingDecision::ChooseCards {
                side: _,           // drawn: selection::Selection, only to the chooser
                source: _,         // drawn: selection's words for the zone
                filter: _,         // engine's: the candidates are listed (view.selection)
                min: _,            // drawn: Prompt::of ("choose 1 to 2 cards")
                max: _,            // drawn: as min
                reveal: _,         // engine's: the reveal is an event the log words
                shuffle_after: _,  // engine's: the shuffle is an event the log words
                destination: _,    // drawn: selection's words for where the cards go
                then: _,           // engine's: what follows the choice
                selected: _,       // drawn: selection's summary, a count to the other seat
                source_card: _,    // drawn: Prompt::of
                prompting_card: _, // drawn: Prompt::of
                source_install: _, // engine's
                resume: _,         // engine's
            } => {}
            PendingDecision::ChooseTriggerOrder {
                chooser: _, // drawn: Prompt::of
                pending: _, // drawn: the buttons, a trigger each
                resume: _,  // engine's
            } => {}
            PendingDecision::ChooseNumber {
                chooser: _,        // drawn: Prompt::card, only to the chooser
                min: _,            // drawn: Prompt::of's detail, the buttons
                max: _,            // drawn: as min
                then: _,           // engine's: what the number is written into
                text: _,           // drawn: Prompt::of's title (the printed clause)
                source_card: _,    // drawn: Prompt::of
                prompting_card: _, // drawn: Prompt::of
                source_install: _, // engine's
                resume: _,         // engine's
                secret: _,         // drawn: the other seat's log reads "Choose in secret" (actions)
            } => {}
            PendingDecision::PsiGame {
                corp_bid: _,       // drawn: Prompt::of — "the Corp is bidding" / "has bid"; the Corp's own "you bid 2"
                corp_max: _,       // drawn: Prompt::of's detail and the buttons, to the Corp
                runner_max: _,     // drawn: as corp_max, to the Runner
                on_match: _,       // engine's: what the outcome resolves (the card's text says it)
                on_differ: _,      // engine's: as on_match
                source_card: _,    // drawn: Prompt::of, Prompt::card
                prompting_card: _, // drawn: as source_card
                source_install: _, // engine's
                resume: _,         // engine's
            } => {}
            PendingDecision::ChooseServer {
                chooser: _,         // drawn: Prompt::of, Prompt::card
                rez_cost_delta: _,  // drawn: once the run starts, as a lingering rez tax (hud::in_effect)
                bonus_run_credits: _, // drawn: once the run starts, beside the Runner's credits (hud::readouts)
                allowed_servers: _, // drawn: the servers lit and offered (ActionMap)
                on_start: _,        // engine's
                install: _,         // drawn: placement::Placement
                move_to_root: _,    // drawn: placement::Placement's question
                remember: _,        // drawn: Prompt::of's "choose a server" and the buttons' "Choose HQ" (actions)
                on_success: _,      // engine's
                source_card: _,     // drawn: Prompt::of
                prompting_card: _,  // drawn: Prompt::of
                source_install: _,  // engine's
                resume: _,          // engine's
            } => {}
        }
    }

    for PublicArchivedCard {
        card: _,     // drawn: Archives' sheet, face or back
        facedown: _, // drawn: Archives' sheet caption, a back to the Runner
    } in archives
    {}
}
