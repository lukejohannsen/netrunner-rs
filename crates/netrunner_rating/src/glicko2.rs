//! Glicko-2 (Glickman, 2013), as published: a rating, a deviation and a
//! volatility per player, updated over a *rating period* of results.
//!
//! Everything here is in the paper's notation so it can be checked
//! against it line by line. The one policy decision is not in this file:
//! `book::RatingBook` uses a rating period of **one game**. Glickman
//! suggests periods of 10-15 games, which keeps the volatility estimate
//! well conditioned; per-game periods are what online play does anyway
//! (nobody waits a fortnight for a rating), and the cost — volatility
//! reacting a little faster — is the right trade for a ladder people and
//! bots read after every match.

use serde::{Deserialize, Serialize};

/// Glicko-2's scale factor between the display scale (1500 ± 350) and the
/// internal one (0 ± 2.01).
const SCALE: f64 = 173.7178;

/// One participant's rating on the display scale.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rating {
    /// The estimate. 1500 for a newcomer.
    pub rating: f64,
    /// How uncertain the estimate is (one standard deviation). 350 for a
    /// newcomer, shrinking with every game and growing with every period
    /// of inactivity.
    pub deviation: f64,
    /// How erratic the participant's results have been. 0.06 for a
    /// newcomer; the update re-estimates it from how surprising each
    /// period's results were.
    pub volatility: f64,
}

impl Default for Rating {
    fn default() -> Self {
        Rating { rating: 1500.0, deviation: 350.0, volatility: 0.06 }
    }
}

impl Rating {
    /// The 95% interval the paper suggests quoting: `rating ± 2·deviation`.
    pub fn interval(&self) -> (f64, f64) {
        (self.rating - 2.0 * self.deviation, self.rating + 2.0 * self.deviation)
    }

    fn mu(&self) -> f64 {
        (self.rating - 1500.0) / SCALE
    }

    fn phi(&self) -> f64 {
        self.deviation / SCALE
    }
}

/// A game's result from one participant's point of view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Score {
    Win,
    Draw,
    Loss,
}

impl Score {
    fn value(self) -> f64 {
        match self {
            Score::Win => 1.0,
            Score::Draw => 0.5,
            Score::Loss => 0.0,
        }
    }

    /// The same game from the other chair.
    pub fn flipped(self) -> Score {
        match self {
            Score::Win => Score::Loss,
            Score::Draw => Score::Draw,
            Score::Loss => Score::Win,
        }
    }
}

/// The system constant τ, which bounds how fast volatility can change.
/// Glickman recommends 0.3-1.2; 0.5 is his worked example's value and the
/// conventional default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Glicko2 {
    pub tau: f64,
}

impl Default for Glicko2 {
    fn default() -> Self {
        Glicko2 { tau: 0.5 }
    }
}

/// Convergence tolerance for the volatility iteration (the paper's ε).
const EPSILON: f64 = 0.000_001;

impl Glicko2 {
    /// `player` after one rating period against `results`: each entry is
    /// an opponent's rating *at the start of the period* and the score
    /// against them. An empty period (no games) leaves the rating and
    /// volatility alone and grows the deviation — step 6 of the paper.
    pub fn update(&self, player: Rating, results: &[(Rating, Score)]) -> Rating {
        let mu = player.mu();
        let phi = player.phi();
        let sigma = player.volatility;

        if results.is_empty() {
            let phi_star = (phi * phi + sigma * sigma).sqrt();
            return Rating { rating: player.rating, deviation: phi_star * SCALE, volatility: sigma };
        }

        // Steps 3 and 4: the estimated variance `v` of the player's rating
        // based only on this period's games, and the estimated improvement
        // `delta`.
        let mut v_inv = 0.0;
        let mut delta_sum = 0.0;
        for (opponent, score) in results {
            let g = g(opponent.phi());
            let e = expected(mu, opponent.mu(), opponent.phi());
            v_inv += g * g * e * (1.0 - e);
            delta_sum += g * (score.value() - e);
        }
        let v = 1.0 / v_inv;
        let delta = v * delta_sum;

        // Step 5: the new volatility, by the Illinois variant of regula
        // falsi the paper specifies.
        let sigma_new = self.new_volatility(sigma, phi, v, delta);

        // Steps 6 and 7.
        let phi_star = (phi * phi + sigma_new * sigma_new).sqrt();
        let phi_new = 1.0 / (1.0 / (phi_star * phi_star) + 1.0 / v).sqrt();
        let mu_new = mu + phi_new * phi_new * delta_sum;

        // Step 8: back to the display scale.
        Rating { rating: mu_new * SCALE + 1500.0, deviation: phi_new * SCALE, volatility: sigma_new }
    }

    fn new_volatility(&self, sigma: f64, phi: f64, v: f64, delta: f64) -> f64 {
        let a = (sigma * sigma).ln();
        let tau = self.tau;
        let f = |x: f64| {
            let ex = x.exp();
            let phi2 = phi * phi;
            let d2 = delta * delta;
            (ex * (d2 - phi2 - v - ex)) / (2.0 * (phi2 + v + ex).powi(2)) - (x - a) / (tau * tau)
        };

        let mut big_a = a;
        let mut big_b = if delta * delta > phi * phi + v {
            (delta * delta - phi * phi - v).ln()
        } else {
            let mut k = 1.0;
            while f(a - k * tau) < 0.0 {
                k += 1.0;
            }
            a - k * tau
        };

        let mut f_a = f(big_a);
        let mut f_b = f(big_b);
        while (big_b - big_a).abs() > EPSILON {
            let big_c = big_a + (big_a - big_b) * f_a / (f_b - f_a);
            let f_c = f(big_c);
            if f_c * f_b <= 0.0 {
                big_a = big_b;
                f_a = f_b;
            } else {
                f_a /= 2.0;
            }
            big_b = big_c;
            f_b = f_c;
        }
        (big_a / 2.0).exp()
    }
}

fn g(phi: f64) -> f64 {
    1.0 / (1.0 + 3.0 * phi * phi / (std::f64::consts::PI * std::f64::consts::PI)).sqrt()
}

fn expected(mu: f64, mu_j: f64, phi_j: f64) -> f64 {
    1.0 / (1.0 + (-g(phi_j) * (mu - mu_j)).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rating(rating: f64, deviation: f64) -> Rating {
        Rating { rating, deviation, volatility: 0.06 }
    }

    /// The worked example from the paper (Glickman, "Example of the
    /// Glicko-2 system", 2013): a 1500 ± 200 player beats a 1400 ± 30, then
    /// loses to a 1550 ± 100 and a 1700 ± 300, with τ = 0.5.
    #[test]
    fn reproduces_glickmans_worked_example() {
        let player = rating(1500.0, 200.0);
        let results = [
            (rating(1400.0, 30.0), Score::Win),
            (rating(1550.0, 100.0), Score::Loss),
            (rating(1700.0, 300.0), Score::Loss),
        ];
        let updated = Glicko2::default().update(player, &results);
        assert!((updated.rating - 1464.06).abs() < 0.01, "rating {}", updated.rating);
        assert!((updated.deviation - 151.52).abs() < 0.01, "deviation {}", updated.deviation);
        assert!((updated.volatility - 0.05999).abs() < 0.00001, "volatility {}", updated.volatility);
    }

    #[test]
    fn an_idle_period_widens_the_deviation_and_changes_nothing_else() {
        let player = rating(1500.0, 200.0);
        let idle = Glicko2::default().update(player, &[]);
        assert_eq!(idle.rating, 1500.0);
        assert_eq!(idle.volatility, 0.06);
        assert!(idle.deviation > 200.0);
        let phi_star = ((200.0f64 / SCALE).powi(2) + 0.06f64 * 0.06).sqrt() * SCALE;
        assert!((idle.deviation - phi_star).abs() < 1e-9);
    }

    #[test]
    fn a_win_raises_and_a_loss_lowers_and_equal_players_drawing_stay_put() {
        let system = Glicko2::default();
        let a = Rating::default();
        let b = Rating::default();
        assert!(system.update(a, &[(b, Score::Win)]).rating > 1500.0);
        assert!(system.update(a, &[(b, Score::Loss)]).rating < 1500.0);
        let drawn = system.update(a, &[(b, Score::Draw)]);
        assert!((drawn.rating - 1500.0).abs() < 1e-9);
        assert!(drawn.deviation < 350.0, "a game played is information even when drawn");
    }

    #[test]
    fn beating_a_stronger_opponent_moves_the_rating_more() {
        let system = Glicko2::default();
        let me = rating(1500.0, 100.0);
        let weak = system.update(me, &[(rating(1300.0, 50.0), Score::Win)]).rating;
        let strong = system.update(me, &[(rating(1700.0, 50.0), Score::Win)]).rating;
        assert!(strong - 1500.0 > weak - 1500.0);
    }

    #[test]
    fn the_interval_is_two_deviations_either_side() {
        assert_eq!(rating(1500.0, 100.0).interval(), (1300.0, 1700.0));
    }
}
