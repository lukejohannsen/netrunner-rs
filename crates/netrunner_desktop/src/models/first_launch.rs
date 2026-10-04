//! The first launch's one question: download the card images?
//!
//! Card scans are off until a person turns them on (`DesktopPrefs::
//! download_images` — a network call on their behalf), and the switch
//! and the download are two screens apart, so somebody new played on
//! text faces without learning there was anything else. The first
//! launch therefore asks, once, after the splash; a no is answered with
//! where the download lives.
//!
//! **Asked once is a flag, written when the person answers.** A window
//! closed on the question has not answered it and is asked again. A
//! person who had already turned the images on is never asked.

use netrunner_client::settings::DesktopPrefs;

/// Whether the question is still owed to this person.
pub fn owed(prefs: &DesktopPrefs) -> bool {
    !prefs.images_offered && !prefs.download_images
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Step {
    /// The question, with its two answers.
    #[default]
    Asking,
    /// Yes: the download is running, and carries on behind the menu.
    Downloading,
    /// No: where and how to download them later.
    Declined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Accept,
    Decline,
    /// On to the menu, from either answer's page.
    Continue,
    /// Escape: "Not now" on the question, Continue after it.
    Escape,
}

/// What the screen does about an intent, beyond redrawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// The settings changed: save them.
    Answered,
    /// The settings changed and the download begins.
    StartDownload,
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Offer {
    pub step: Step,
}

/// What the person is told when they say no, and the only place the way
/// to the images is written out.
pub const LATER: &str = "Cards are drawn as text until you download their images. You can do that at any time: turn on \"Download card images\" in Settings, then press \"Download images\" on the Cards screen.";

impl Offer {
    pub fn apply(&mut self, intent: Intent, prefs: &mut DesktopPrefs) -> Outcome {
        match (self.step, intent) {
            (Step::Asking, Intent::Accept) => {
                prefs.images_offered = true;
                prefs.download_images = true;
                self.step = Step::Downloading;
                Outcome::StartDownload
            }
            (Step::Asking, Intent::Decline | Intent::Escape) => {
                prefs.images_offered = true;
                self.step = Step::Declined;
                Outcome::Answered
            }
            (Step::Asking, Intent::Continue) => Outcome::Nothing,
            (_, Intent::Continue | Intent::Escape) => Outcome::Leave,
            (_, Intent::Accept | Intent::Decline) => Outcome::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes_turns_the_images_on_and_starts_the_download() {
        let mut prefs = DesktopPrefs::default();
        assert!(owed(&prefs));
        let mut offer = Offer::default();
        assert_eq!(offer.apply(Intent::Accept, &mut prefs), Outcome::StartDownload);
        assert!(prefs.download_images && prefs.images_offered);
        assert_eq!(offer.step, Step::Downloading);
        assert!(!owed(&prefs));
        assert_eq!(offer.apply(Intent::Continue, &mut prefs), Outcome::Leave);
    }

    #[test]
    fn no_is_remembered_and_leaves_the_images_off() {
        for answer in [Intent::Decline, Intent::Escape] {
            let mut prefs = DesktopPrefs::default();
            let mut offer = Offer::default();
            assert_eq!(offer.apply(answer, &mut prefs), Outcome::Answered);
            assert!(!prefs.download_images && prefs.images_offered);
            assert_eq!(offer.step, Step::Declined, "a no is answered with where the download lives");
            assert!(!owed(&prefs));
            assert_eq!(offer.apply(Intent::Escape, &mut prefs), Outcome::Leave);
        }
    }

    /// Somebody who found the switch themselves is not asked about it.
    #[test]
    fn images_already_on_are_never_offered() {
        let prefs = DesktopPrefs { download_images: true, ..Default::default() };
        assert!(!owed(&prefs));
    }

    /// The question cannot be left without an answer, and an answer is
    /// given once.
    #[test]
    fn the_question_takes_only_an_answer() {
        let mut prefs = DesktopPrefs::default();
        let mut offer = Offer::default();
        assert_eq!(offer.apply(Intent::Continue, &mut prefs), Outcome::Nothing);
        assert!(owed(&prefs));
        offer.apply(Intent::Decline, &mut prefs);
        assert_eq!(offer.apply(Intent::Accept, &mut prefs), Outcome::Nothing);
        assert!(!prefs.download_images);
    }
}
