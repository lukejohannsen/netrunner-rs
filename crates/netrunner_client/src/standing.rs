//! A card's "you may", answered once: **Always**, **Never**, or ask as
//! before (Phase 7 §8 item 14, from jinteki's autoresolve, whose FAQ's
//! quietest complaint is the same prompt answered the same way forty times
//! a game — here, The Zwicky Group's "draw 1 card" and Cookbook's counter).
//!
//! **A client policy, not a rule — `play::lone_pass`'s shape.** The engine
//! still parks every prompt and hears every answer; a client that holds a
//! standing answer for the prompt submits the one of `legal_actions` it
//! names instead of asking. So nothing here is a new action: `Session`,
//! the bots, the sweeps and `ActionSpace` never see it.
//!
//! **What counts is read off the card, not guessed from the prompt.** The
//! engine has no `optional` flag: a printed "you may" is a
//! `PresentChoice` whose declined option is an empty `Sequence` with the
//! text `""` (the Linked Clause Rule). A prompt is an optional trigger
//! when it is the viewer's own, its card is named, exactly one of its
//! texts is that `""`, and the card has a trigger other than `OnPlay`
//! holding a `PresentChoice` with exactly those texts. The last clause
//! leaves out an event's own "may" (the person just chose to play it),
//! an activated ability's, and the choices the engine builds by hand,
//! whose texts are empty.
//!
//! **Paid choices always ask** (decided with the person, 23 September
//! 2026). "You may pay 1[credit] to prevent 1 net damage" and "you may
//! remove 1 hosted agenda counter" spend something the person may want to
//! keep for later, and an answer given once would spend it every time the
//! cost happened to be affordable.
//!
//! **The key is the card and its printed clauses** (`PromptKey`), never a
//! trigger's index: the words are what the person answered, and they are
//! what the view carries. So Dewi Subrotoputri's two triggers are two
//! answers, and Pantograph's scored and stolen triggers — one printed
//! sentence, one question — share one. **Always** needs exactly one
//! option to say yes to: Mitra Aman's swap offers two, so it can only be
//! told Never.
//!
//! **An answer is kept across games**, in the settings file both clients
//! share (`Settings::answers`), and a settings screen in each lists them
//! so any one can go back to asking. A match-only answer, as jinteki
//! keeps, was the alternative, and put the same forty questions back at
//! the start of every game.

use serde::{Deserialize, Serialize};

use netrunner_core::cards::CardRegistry;
use netrunner_core::dsl::{CardId, Effect, Trigger};
use netrunner_core::rules::{PendingDecision, PlayerAction};
use netrunner_core::view::ClientView;

/// What the person said a prompt's answer always is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    Always,
    Never,
}

impl Answer {
    /// The word on the button and in the log.
    pub fn label(self) -> &'static str {
        match self {
            Answer::Always => "Always",
            Answer::Never => "Never",
        }
    }
}

/// Which prompt an answer is for: the card, and the printed clauses of
/// every option but the declined one, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptKey {
    pub card: CardId,
    pub clauses: Vec<String>,
}

/// A prompt that may be answered once for good.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionalPrompt {
    pub key: PromptKey,
    /// The one option to say yes to, when there is exactly one.
    pub yes: Option<usize>,
    /// The declined option.
    pub no: usize,
}

impl OptionalPrompt {
    /// The option `answer` stands for, if the prompt has one.
    pub fn option(&self, answer: Answer) -> Option<usize> {
        match answer {
            Answer::Always => self.yes,
            Answer::Never => Some(self.no),
        }
    }

    /// The answers a person may give it, in the order they are offered.
    pub fn offered(&self) -> Vec<Answer> {
        [Answer::Always, Answer::Never].into_iter().filter(|answer| self.option(*answer).is_some()).collect()
    }

    /// The action that gives `answer`.
    pub fn action(&self, answer: Answer) -> Option<PlayerAction> {
        self.option(answer).map(|option_index| PlayerAction::ResolvePendingChoice { option_index })
    }
}

/// One remembered answer, as the settings file stores it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandingAnswer {
    #[serde(flatten)]
    pub key: PromptKey,
    pub answer: Answer,
}

/// Every remembered answer. A list rather than a map because the key is
/// not a string, and a JSON object's keys have to be.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Answers(pub Vec<StandingAnswer>);

impl Answers {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, key: &PromptKey) -> Option<Answer> {
        self.0.iter().find(|entry| entry.key == *key).map(|entry| entry.answer)
    }

    /// Remember `answer` for `key`, or with `None` go back to asking.
    pub fn set(&mut self, key: PromptKey, answer: Option<Answer>) {
        self.0.retain(|entry| entry.key != key);
        if let Some(answer) = answer {
            self.0.push(StandingAnswer { key, answer });
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &StandingAnswer> {
        self.0.iter()
    }
}

/// The prompt `view` is parked on, if it is the viewer's own optional
/// trigger (the module comment's four conditions).
pub fn optional_trigger(view: &ClientView, registry: &CardRegistry) -> Option<OptionalPrompt> {
    let Some(PendingDecision::ChooseEffect { chooser, option_texts, source_card: Some(card), .. }) = &view.pending_decision else {
        return None;
    };
    if Some(*chooser) != view.viewer.side() {
        return None;
    }
    let mut declined = option_texts.iter().enumerate().filter(|(_, text)| text.is_empty()).map(|(index, _)| index);
    let no = declined.next()?;
    if declined.next().is_some() {
        return None;
    }
    let definition = registry.get(card)?;
    let printed_on_a_trigger = definition.triggers.iter().filter(|trigger| trigger.trigger != Trigger::OnPlay).any(|trigger| {
        let mut found = false;
        for effect in &trigger.effects {
            effect.for_each_effect(&mut |effect| {
                if let Effect::PresentChoice { texts, .. } = effect {
                    found |= texts == option_texts;
                }
            });
        }
        found
    });
    if !printed_on_a_trigger {
        return None;
    }
    let yeses: Vec<usize> = (0..option_texts.len()).filter(|index| *index != no).collect();
    let clauses = yeses.iter().map(|index| option_texts[*index].clone()).collect();
    Some(OptionalPrompt { key: PromptKey { card: card.clone(), clauses }, yes: (yeses.len() == 1).then(|| yeses[0]), no })
}

/// The action a client takes for the person without asking, and the
/// answer it stands for: `Some` exactly when `view` is parked on an
/// optional trigger the person has answered for good and the action is
/// one `view.legal_actions` lists. A lesson does not use it, as it does
/// not use the lone pass.
pub fn standing_answer(view: &ClientView, registry: &CardRegistry, answers: &Answers) -> Option<(PlayerAction, Answer)> {
    let prompt = optional_trigger(view, registry)?;
    let answer = answers.get(&prompt.key)?;
    let action = prompt.action(answer)?;
    view.legal_actions.contains(&action).then_some((action, answer))
}

/// The log line for an answer given without asking — "Always: Cookbook —
/// place 1 virus counter on it" — so the person sees it happen.
pub fn log_line(key: &PromptKey, answer: Answer, registry: &CardRegistry) -> String {
    format!("{}: {}", answer.label(), describe(key, registry))
}

/// The card's name and what it offers, as a settings list shows an
/// answer.
pub fn describe(key: &PromptKey, registry: &CardRegistry) -> String {
    let title = registry.get(&key.card).map_or_else(|| key.card.0.clone(), |card| card.title.clone());
    format!("{title} — {}", key.clauses.join(" / "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use netrunner_core::rules::{GameState, PendingChoiceResume, PendingPaidChoice, Side, Viewer};
    use netrunner_core::view::build_client_view;

    fn registry() -> CardRegistry {
        let mut registry = CardRegistry::new();
        netrunner_core::cards::register_playable_cards(&mut registry);
        registry
    }

    /// The `nth` "may" `card` prints on any of its triggers, `OnPlay`
    /// included, and whose it is.
    fn printed_may(card: &str, nth: usize, registry: &CardRegistry) -> (Side, Vec<Effect>, Vec<String>) {
        let definition = registry.get(&CardId(card.to_string())).expect("a playable card");
        let mut found = Vec::new();
        for trigger in &definition.triggers {
            for effect in &trigger.effects {
                effect.for_each_effect(&mut |effect| {
                    if let Effect::PresentChoice { chooser, options, texts } = effect
                        && texts.iter().any(String::is_empty)
                    {
                        found.push((*chooser, options.clone(), texts.clone()));
                    }
                });
            }
        }
        found.swap_remove(nth)
    }

    /// A view seen by `viewer`, parked on `card`'s `nth` "may", with every
    /// option legal.
    fn parked_on(card: &str, nth: usize, viewer: Side, registry: &CardRegistry) -> ClientView {
        let (chooser, options, option_texts) = printed_may(card, nth, registry);
        let mut view = build_client_view(&GameState::new(7), registry, Viewer::Player(viewer));
        view.legal_actions = (0..options.len()).map(|option_index| PlayerAction::ResolvePendingChoice { option_index }).collect();
        view.pending_decision = Some(PendingDecision::ChooseEffect {
            chooser,
            options,
            option_texts,
            source_card: Some(CardId(card.to_string())),
            prompting_card: None,
            source_install: None,
            resume: PendingChoiceResume::None,
        });
        view
    }

    #[test]
    fn a_triggered_may_can_be_answered_either_way_and_the_answer_is_taken() {
        let registry = registry();
        let view = parked_on("cookbook", 0, Side::Runner, &registry);
        let prompt = optional_trigger(&view, &registry).expect("Cookbook's counter is an optional trigger");
        assert_eq!(prompt.offered(), vec![Answer::Always, Answer::Never]);
        assert_eq!(prompt.key.clauses, vec!["place 1 virus counter on it".to_string()]);

        let mut answers = Answers::default();
        assert_eq!(standing_answer(&view, &registry, &answers), None, "nothing is taken until the person answers");
        answers.set(prompt.key.clone(), Some(Answer::Always));
        assert_eq!(standing_answer(&view, &registry, &answers), Some((PlayerAction::ResolvePendingChoice { option_index: 0 }, Answer::Always)));
        answers.set(prompt.key.clone(), Some(Answer::Never));
        assert_eq!(answers.iter().count(), 1, "a second answer replaces the first");
        assert_eq!(standing_answer(&view, &registry, &answers), Some((PlayerAction::ResolvePendingChoice { option_index: 1 }, Answer::Never)));
        answers.set(prompt.key, None);
        assert!(answers.is_empty(), "Ask forgets it");
    }

    /// Two ways to say yes is no one thing for Always to mean.
    #[test]
    fn a_may_with_two_yeses_can_only_be_told_never() {
        let registry = registry();
        let (_, _, texts) = printed_may("mitra_aman", 1, &registry);
        assert_eq!(texts.len(), 3, "the fixture is the swap, with two options and a decline");
        let view = parked_on("mitra_aman", 1, Side::Corp, &registry);
        let prompt = optional_trigger(&view, &registry).expect("Mitra Aman's swap is an optional trigger");
        assert_eq!(prompt.offered(), vec![Answer::Never]);
        let mut answers = Answers::default();
        answers.set(prompt.key, Some(Answer::Always));
        assert_eq!(standing_answer(&view, &registry, &answers), None, "an Always it cannot mean is not taken");
    }

    /// An event's "may" was chosen by playing the event; the opponent's
    /// prompt is not the viewer's to answer.
    #[test]
    fn an_event_and_an_opponents_prompt_always_ask() {
        let registry = registry();
        assert_eq!(optional_trigger(&parked_on("scrounge", 0, Side::Runner, &registry), &registry), None);
        assert_eq!(optional_trigger(&parked_on("cookbook", 0, Side::Corp, &registry), &registry), None);
    }

    /// A card the viewer cannot see names no prompt to remember.
    #[test]
    fn a_masked_card_always_asks() {
        let registry = registry();
        let mut view = parked_on("mitra_aman", 0, Side::Corp, &registry);
        if let Some(PendingDecision::ChooseEffect { source_card, .. }) = &mut view.pending_decision {
            *source_card = None;
        }
        assert_eq!(optional_trigger(&view, &registry), None);
    }

    /// A paid "may" is a different prompt and never qualifies (the module
    /// comment's decision), even with an answer stored under its card.
    #[test]
    fn a_paid_may_always_asks() {
        let registry = registry();
        let mut view = build_client_view(&GameState::new(7), &registry, Viewer::Player(Side::Runner));
        view.pending_paid_choice = Some(PendingPaidChoice {
            side: Side::Runner,
            cost: netrunner_core::dsl::Cost::Credits(1),
            if_paid: Effect::Sequence(Vec::new()),
            if_declined: Effect::Sequence(Vec::new()),
            text: Some("you may pay 1[credit] to prevent 1 net damage".to_string()),
            source_card: Some(CardId("net_shield".to_string())),
            prompting_card: None,
            source_install: None,
            resume: netrunner_core::rules::PendingPaidChoiceResume::None,
        });
        assert_eq!(optional_trigger(&view, &registry), None);
    }

    /// An answer is only ever an action the engine listed.
    #[test]
    fn an_answer_is_taken_only_when_it_is_legal() {
        let registry = registry();
        let mut view = parked_on("cookbook", 0, Side::Runner, &registry);
        let prompt = optional_trigger(&view, &registry).unwrap();
        let mut answers = Answers::default();
        answers.set(prompt.key, Some(Answer::Always));
        view.legal_actions.retain(|action| *action != PlayerAction::ResolvePendingChoice { option_index: 0 });
        assert_eq!(standing_answer(&view, &registry, &answers), None);
    }

    #[test]
    fn the_file_stores_an_answer_by_card_and_clause() {
        let mut answers = Answers::default();
        answers.set(PromptKey { card: CardId("cookbook".to_string()), clauses: vec!["place 1 virus counter on it".to_string()] }, Some(Answer::Always));
        let json = serde_json::to_string(&answers).unwrap();
        assert_eq!(json, r#"[{"card":"cookbook","clauses":["place 1 virus counter on it"],"answer":"Always"}]"#);
        assert_eq!(serde_json::from_str::<Answers>(&json).unwrap(), answers);
    }
}
