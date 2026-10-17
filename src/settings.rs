// ~~~ settings.rs ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
//
// Lives in the `settings` table of the database
// (`myoso --db other.db` brings its own settings along, and a copied
// database keeps them) and are read through `Store::settings()`
//
// Settings are `Copy` on purpose, `Store` keeps them in a `Cell`, so hot paths
// (review recording, session building) can read them without borrow juggling

use crate::models::SessionLimits;

/// selectable ceilings for the maximum review interval, in days
/// `36_500` (100 years) means "no limit"
pub const MAX_INTERVAL_LADDER: [u32; 7] = [30, 90, 180, 365, 730, 1825, 36_500];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// new cards introduced per session (0 = none)
    pub new_cards: usize,
    /// review cards per session
    pub max_reviews: usize,

    // ~~ scheduling ~~
    /// FSRS target recall probability at each scheduled review
    pub desired_retention: f64,
    /// no step is ever scheduled further out than this many days
    pub max_interval_days: u32,
    /// Hard (2) re-locks the steps below it not just Again (1)
    pub relock_on_hard: bool,

    // ~~ weak steps ~~
    /// show the "this step is a leech" prompt at all
    pub leech_warnings: bool,
    /// consecutive Again ratings that make a step a leech
    pub leech_threshold: u32,

    // ~~ review & interface ~~
    /// off = pressing 4 counts as 3
    pub allow_easy: bool,
    /// mouse clicks and wheel. Off leaves the terminal's native text selection alone
    pub mouse: bool,
}

impl Default for Settings {
    /// every default reproduces what was previously hardcoded, so a user who
    /// never opens the settings screen sees no change in behaviour
    fn default() -> Self {
        Self {
            new_cards:         20,
            max_reviews:       200,
            desired_retention: 0.90,
            max_interval_days: 36_500,
            relock_on_hard:    true,
            leech_warnings:    true,
            leech_threshold:   8,
            allow_easy:        true,
            mouse:             true,
        }
    }
}

impl Settings {
    /// force every field back inside its legal range
    /// used after loading from disk (hand-edited, newer-version values) and
    /// after every adjustment, so no other code needs to re-validate
    pub fn clamped(mut self) -> Self {
        let d = Settings::default();
        self.new_cards = self.new_cards.min(999);
        self.max_reviews = self.max_reviews.clamp(1, 9999);
        self.desired_retention = if self.desired_retention.is_finite() {
            self.desired_retention.clamp(0.70, 0.97)
        } else {
            d.desired_retention
        };
        self.max_interval_days = self.max_interval_days.clamp(30, 36_500);
        self.leech_threshold = self.leech_threshold.clamp(4, 20);
        self
    }

    pub fn session_limits(&self) -> SessionLimits {
        SessionLimits { max_reviews: self.max_reviews, new_cards: self.new_cards }
    }

    /// size of the recent-history window for the oscillating Again, Hard leech
    /// scales with the threshold, and gives the original 8 / 10 at default
    pub fn leech_lookback(&self) -> usize {
        self.leech_threshold as usize + 2
    }

    /// again always re-locks, hard only when `relock_on_hard` is on
    pub fn relock_max_rating(&self) -> u8 {
        if self.relock_on_hard { 2 } else { 1 }
    }
}

// ~~~ Setting ids ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingId {
    NewCards,
    MaxReviews,
    DesiredRetention,
    MaxInterval,
    RelockOnHard,
    LeechWarnings,
    LeechThreshold,
    AllowEasy,
    Mouse,
}

/// the factor applied to a step when the user holds the "coarse" modifier
const COARSE: usize = 5;

fn step_usize(v: usize, dir: i32, step: usize, coarse: bool) -> usize {
    let d = step * if coarse { COARSE } else { 1 };
    if dir < 0 { v.saturating_sub(d) } else { v.saturating_add(d) }
}

impl SettingId {
    /// display order, top to bottom
    pub const ALL: [SettingId; 9] = [
        SettingId::NewCards,
        SettingId::MaxReviews,
        SettingId::DesiredRetention,
        SettingId::MaxInterval,
        SettingId::RelockOnHard,
        SettingId::LeechWarnings,
        SettingId::LeechThreshold,
        SettingId::AllowEasy,
        SettingId::Mouse,
    ];

    pub fn section(self) -> &'static str {
        match self {
            Self::NewCards | Self::MaxReviews => "Sessions",
            Self::DesiredRetention | Self::MaxInterval | Self::RelockOnHard => "Scheduling",
            Self::LeechWarnings | Self::LeechThreshold => "Weak steps",
            Self::AllowEasy | Self::Mouse => "Interface",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::NewCards         => "New cards per session",
            Self::MaxReviews       => "Max reviews per session",
            Self::DesiredRetention => "Target retention",
            Self::MaxInterval      => "Maximum interval",
            Self::RelockOnHard     => "Hard re-locks later steps",
            Self::LeechWarnings    => "Leech warnings",
            Self::LeechThreshold   => "Leech after N Agains",
            Self::AllowEasy        => "Allow the Easy rating",
            Self::Mouse            => "Mouse support",
        }
    }

    /// one or two sentences shown under the list while the row is selected
    pub fn help(self) -> &'static str {
        match self {
            Self::NewCards =>
                "How many never-seen cards a review session may introduce. Daily cards are not limited. Set 0 to review only.",
            Self::MaxReviews =>
                "Cap on already-seen cards per session, most-forgotten first. The rest stay due for next time.",
            Self::DesiredRetention =>
                "The recall chance FSRS aims for when a step comes back. Higher = more reviews, fewer lapses. 90% is the standard.",
            Self::MaxInterval =>
                "No step is scheduled further out than this. Useful for exam material that must stay fresh. Does not change a step's stability.",
            Self::RelockOnHard =>
                "On: rating a step Hard (2) makes every step below it due again, like Again. Off: only Again does this.",
            Self::LeechWarnings =>
                "Warn when a step keeps failing and offer to split it into two simpler steps.",
            Self::LeechThreshold =>
                "Consecutive Again ratings that flag a step as a leech.",
            Self::AllowEasy =>
                "Off: pressing [4] is treated as [3]. Easy inflates intervals quickly, so it is best kept for rare cases.",
            Self::Mouse =>
                "Click and scroll in the UI. Turn off to use your terminal's own text selection. Applies immediately.",
        }
    }

    /// stable key in the `settings` table.
    /// Note: never rename one, it would orphan the stored value
    pub fn key(self) -> &'static str {
        match self {
            Self::NewCards         => "new_cards",
            Self::MaxReviews       => "max_reviews",
            Self::DesiredRetention => "desired_retention",
            Self::MaxInterval      => "max_interval_days",
            Self::RelockOnHard     => "relock_on_hard",
            Self::LeechWarnings    => "leech_warnings",
            Self::LeechThreshold   => "leech_threshold",
            Self::AllowEasy        => "allow_easy",
            Self::Mouse            => "mouse",
        }
    }

    pub fn is_bool(self) -> bool {
        matches!(
            self,
            Self::RelockOnHard | Self::LeechWarnings | Self::AllowEasy | Self::Mouse
        )
    }

    /// a row can be greyed out when another setting makes it irrelevant
    pub fn is_active(self, s: &Settings) -> bool {
        match self {
            Self::LeechThreshold => s.leech_warnings,
            _ => true,
        }
    }

    pub fn is_default(self, s: &Settings) -> bool {
        self.encode(s) == self.encode(&Settings::default())
    }

    /// the value as shown on screen
    pub fn value_text(self, s: &Settings) -> String {
        let onoff = |b: bool| if b { "on" } else { "off" }.to_string();
        match self {
            Self::NewCards         => s.new_cards.to_string(),
            Self::MaxReviews       => s.max_reviews.to_string(),
            Self::DesiredRetention => format!("{:.0}%", s.desired_retention * 100.0),
            Self::MaxInterval      => match s.max_interval_days {
                30     => "1 month".into(),
                90     => "3 months".into(),
                180    => "6 months".into(),
                365    => "1 year".into(),
                730    => "2 years".into(),
                1825   => "5 years".into(),
                36_500 => "no limit".into(),
                n      => format!("{n} days"),
            },
            Self::RelockOnHard     => onoff(s.relock_on_hard),
            Self::LeechWarnings    => onoff(s.leech_warnings),
            Self::LeechThreshold   => s.leech_threshold.to_string(),
            Self::AllowEasy        => onoff(s.allow_easy),
            Self::Mouse            => onoff(s.mouse),
        }
    }

    /// step the value one notch (`dir` < 0 down, > 0 up). Booleans flip on
    /// either direction. `coarse` takes bigger steps on numeric rows
    pub fn adjust(self, s: &mut Settings, dir: i32, coarse: bool) {
        match self {
            Self::NewCards   => s.new_cards   = step_usize(s.new_cards,   dir, 5,  coarse),
            Self::MaxReviews => s.max_reviews = step_usize(s.max_reviews, dir, 25, coarse),
            Self::DesiredRetention => {
                let d = if coarse { 0.05 } else { 0.01 };
                let v = s.desired_retention + if dir < 0 { -d } else { d };
                // round to whole percent so repeated steps never drift (0.8999999)
                s.desired_retention = (v * 100.0).round() / 100.0;
            }
            Self::MaxInterval => {
                let last = MAX_INTERVAL_LADDER.len() as i32 - 1;
                let cur = MAX_INTERVAL_LADDER
                    .iter()
                    .position(|&v| v >= s.max_interval_days)
                    .unwrap_or(last as usize) as i32;
                let next = (cur + if dir < 0 { -1 } else { 1 }).clamp(0, last);
                s.max_interval_days = MAX_INTERVAL_LADDER[next as usize];
            }
            Self::LeechThreshold => {
                let d = if coarse { COARSE as u32 } else { 1 };
                s.leech_threshold = if dir < 0 {
                    s.leech_threshold.saturating_sub(d)
                } else {
                    s.leech_threshold.saturating_add(d)
                };
            }
            Self::RelockOnHard  => s.relock_on_hard = !s.relock_on_hard,
            Self::LeechWarnings => s.leech_warnings = !s.leech_warnings,
            Self::AllowEasy     => s.allow_easy     = !s.allow_easy,
            Self::Mouse         => s.mouse          = !s.mouse,
        }
        *s = s.clamped();
    }

    pub fn reset(self, s: &mut Settings) {
        let d = Settings::default();
        match self {
            Self::NewCards         => s.new_cards = d.new_cards,
            Self::MaxReviews       => s.max_reviews = d.max_reviews,
            Self::DesiredRetention => s.desired_retention = d.desired_retention,
            Self::MaxInterval      => s.max_interval_days = d.max_interval_days,
            Self::RelockOnHard     => s.relock_on_hard = d.relock_on_hard,
            Self::LeechWarnings    => s.leech_warnings = d.leech_warnings,
            Self::LeechThreshold   => s.leech_threshold = d.leech_threshold,
            Self::AllowEasy        => s.allow_easy = d.allow_easy,
            Self::Mouse            => s.mouse = d.mouse,
        }
    }

    // ~~ persistence ~~

    /// the value as stored in the `settings` table
    pub fn encode(self, s: &Settings) -> String {
        let b = |v: bool| if v { "1" } else { "0" }.to_string();
        match self {
            Self::NewCards         => s.new_cards.to_string(),
            Self::MaxReviews       => s.max_reviews.to_string(),
            Self::DesiredRetention => format!("{:.2}", s.desired_retention),
            Self::MaxInterval      => s.max_interval_days.to_string(),
            Self::RelockOnHard     => b(s.relock_on_hard),
            Self::LeechWarnings    => b(s.leech_warnings),
            Self::LeechThreshold   => s.leech_threshold.to_string(),
            Self::AllowEasy        => b(s.allow_easy),
            Self::Mouse            => b(s.mouse),
        }
    }

    /// parse a stored value into `s`. Malformed input is ignored, leaving the
    /// field at whatever it already was (the default on first load)
    pub fn decode(self, s: &mut Settings, raw: &str) {
        let raw = raw.trim();
        let flag = || match raw {
            "1" | "true"  => Some(true),
            "0" | "false" => Some(false),
            _ => None,
        };
        match self {
            Self::NewCards         => if let Ok(v) = raw.parse() { s.new_cards = v },
            Self::MaxReviews       => if let Ok(v) = raw.parse() { s.max_reviews = v },
            Self::DesiredRetention => if let Ok(v) = raw.parse() { s.desired_retention = v },
            Self::MaxInterval      => if let Ok(v) = raw.parse() { s.max_interval_days = v },
            Self::LeechThreshold   => if let Ok(v) = raw.parse() { s.leech_threshold = v },
            Self::RelockOnHard     => if let Some(v) = flag() { s.relock_on_hard = v },
            Self::LeechWarnings    => if let Some(v) = flag() { s.leech_warnings = v },
            Self::AllowEasy        => if let Some(v) = flag() { s.allow_easy = v },
            Self::Mouse            => if let Some(v) = flag() { s.mouse = v },
        }
    }

    pub fn from_key(key: &str) -> Option<SettingId> {
        Self::ALL.iter().copied().find(|id| id.key() == key)
    }
}

// ~~~ Tests ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_previously_hardcoded_behaviour() {
        let d = Settings::default();
        assert_eq!(d.session_limits().max_reviews, 200);
        assert_eq!(d.session_limits().new_cards, 20);
        assert_eq!(d.leech_threshold, 8);
        assert_eq!(d.leech_lookback(), 10);
        assert_eq!(d.relock_max_rating(), 2);
        assert!((d.desired_retention - 0.90).abs() < 1e-12);
        assert_eq!(d, d.clamped(), "defaults must already be legal");
    }

    #[test]
    fn every_setting_round_trips_through_its_stored_form() {
        let mut odd = Settings::default();
        odd.new_cards = 0;
        odd.max_reviews = 75;
        odd.desired_retention = 0.85;
        odd.max_interval_days = 365;
        odd.relock_on_hard = false;
        odd.leech_warnings = false;
        odd.leech_threshold = 12;
        odd.allow_easy = false;
        odd.mouse = false;

        let mut loaded = Settings::default();
        for id in SettingId::ALL {
            id.decode(&mut loaded, &id.encode(&odd));
        }
        assert_eq!(loaded.clamped(), odd);
    }

    #[test]
    fn keys_are_unique_and_resolve_back() {
        for id in SettingId::ALL {
            assert_eq!(SettingId::from_key(id.key()), Some(id));
        }
        let mut keys: Vec<_> = SettingId::ALL.iter().map(|i| i.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), SettingId::ALL.len());
    }

    #[test]
    fn garbage_and_out_of_range_values_are_survivable() {
        let mut s = Settings::default();
        SettingId::NewCards.decode(&mut s, "lots");
        SettingId::Mouse.decode(&mut s, "maybe");
        assert_eq!(s, Settings::default());

        SettingId::DesiredRetention.decode(&mut s, "7.5");
        SettingId::LeechThreshold.decode(&mut s, "0");
        SettingId::MaxReviews.decode(&mut s, "0");
        let s = s.clamped();
        assert!((s.desired_retention - 0.97).abs() < 1e-12);
        assert_eq!(s.leech_threshold, 4);
        assert_eq!(s.max_reviews, 1);

        let mut nan = Settings::default();
        nan.desired_retention = f64::NAN;
        assert!((nan.clamped().desired_retention - 0.90).abs() < 1e-12);
    }

    #[test]
    fn adjusting_stays_in_range_and_does_not_drift() {
        let mut s = Settings::default();
        for _ in 0..40 {
            SettingId::DesiredRetention.adjust(&mut s, -1, false);
        }
        assert!((s.desired_retention - 0.70).abs() < 1e-12);
        for _ in 0..40 {
            SettingId::DesiredRetention.adjust(&mut s, 1, false);
        }
        assert!((s.desired_retention - 0.97).abs() < 1e-12);

        let mut s = Settings::default();
        for _ in 0..10 {
            SettingId::NewCards.adjust(&mut s, -1, true);
        }
        assert_eq!(s.new_cards, 0, "stops at zero rather than wrapping");
    }

    #[test]
    fn max_interval_walks_the_ladder_both_ways() {
        let mut s = Settings::default();
        assert_eq!(SettingId::MaxInterval.value_text(&s), "no limit");
        SettingId::MaxInterval.adjust(&mut s, -1, false);
        assert_eq!(s.max_interval_days, 1825);
        SettingId::MaxInterval.adjust(&mut s, 1, false);
        SettingId::MaxInterval.adjust(&mut s, 1, false);
        assert_eq!(s.max_interval_days, 36_500, "clamped at the top");
        s.max_interval_days = 400; // hand-edited, between rungs
        SettingId::MaxInterval.adjust(&mut s, -1, false);
        assert_eq!(s.max_interval_days, 365);
    }

    #[test]
    fn booleans_flip_either_direction_and_reset_restores_defaults() {
        let mut s = Settings::default();
        SettingId::AllowEasy.adjust(&mut s, -1, false);
        assert!(!s.allow_easy);
        SettingId::AllowEasy.adjust(&mut s, -1, false);
        assert!(s.allow_easy);

        SettingId::LeechThreshold.adjust(&mut s, 1, true);
        assert!(!SettingId::LeechThreshold.is_default(&s));
        SettingId::LeechThreshold.reset(&mut s);
        assert!(SettingId::LeechThreshold.is_default(&s));
    }

    #[test]
    fn relock_rating_follows_the_toggle() {
        let mut s = Settings::default();
        s.relock_on_hard = false;
        assert_eq!(s.relock_max_rating(), 1);
    }
}
