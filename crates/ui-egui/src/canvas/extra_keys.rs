//! Extra keyboard shortcuts (Preferences ▸ Documents and view ▸ Extra keyboard shortcuts).
//!
//! An opt-in set of plain, single-character keys for moving around a document without the mouse,
//! in the style of terminal pagers and keyboard-driven viewers. They come *beside* the keys
//! PdfCraft already has (arrows, Page Up / Page Down, Home / End, the ⌘ / Ctrl chords), never
//! instead of them, and do nothing while the preference is off.
//!
//! | Key | Action |
//! |---|---|
//! | `h` `j` `k` `l` | scroll left, down, up, right |
//! | Space, `b` | forward, back one screenful |
//! | `,` `.` | previous, next page |
//! | `g`, `<n>g` | first page, page *n* |
//! | `G`, `<n>G` | last page, page *n* |
//! | `/`, `?` | search forward, backward in a bar along the bottom (not the ⌘F find bar) |
//! | `n`, `N` | next match in the direction of the search, and the other way |
//! | `m`, `t`, `T` | remember this place in the view history; previous view; next view |
//! | `<n>m`, `<n>t` | save this page as bookmark *n*; go to bookmark *n* |
//! | `W` `H` `F` | fit width, height, page (`Z` also fits the page) |
//! | `<n>z` | zoom to *n* percent |
//! | `+` `-` | zoom in, out |
//! | `[` `]` | rotate the view counter-clockwise, clockwise |
//! | `I` | invert colours (Reading colours: white on black) |
//!
//! Each action's key can be changed in Preferences ▸ Customize keys: [`Bindings`] holds the
//! choices, and an action left alone keeps the keys above.
//!
//! A number typed first (`12g`) applies to the next key and is dropped after a few seconds idle.
//!
//! Space was the Hand tool when held; with this preference on it goes forward one screenful instead.
//! The Hand tool is on `p` whether or not this preference is on (and in the toolbar), so `h` is free
//! to scroll left. Nothing else changes.
//!
//! The keys are typed characters, so they follow the keyboard layout. They are read through
//! [`PageKeys`] (nothing while a dialog, the palette or a menu is over the pages) and only when no
//! text field, form field or page text editor has the keyboard.

use std::collections::BTreeMap;

use serde_json::Value;

use super::navigation::{KEY_SCROLL_LINE, step_vertical};
use super::scroll_speed::ScrollSpeed;
use super::{DocView, Fit, ViewAction};
use crate::page_keys::PageKeys;

/// The largest number taken before a key (`150z`, `12g`): more digits stop at this.
const MAX_COUNT: u32 = 999_999;
/// A number typed before a key is dropped after this many seconds without another key.
const COUNT_SECONDS: f64 = 3.0;
/// Numbered bookmarks kept for one document.
const MAX_MARKS: usize = 64;
/// How long the view history grows (as in `DocView::go_to_page`).
const MAX_HISTORY: usize = 200;
/// Keys the tools already use (V Select, P Hand, S sticky note, U highlight, D draw): not for these
/// actions.
const RESERVED: [char; 5] = ['p', 'v', 's', 'u', 'd'];

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    ScrollLeft,
    ScrollRight,
    ScrollDown,
    ScrollUp,
    ScreenForward,
    ScreenBack,
    PrevPage,
    NextPage,
    /// First page, or page `n`.
    GoFirst,
    /// Last page, or page `n`.
    GoLast,
    /// The search bar along the bottom, searching forward.
    SearchForward,
    /// The search bar along the bottom, searching backward.
    SearchBackward,
    /// The next match, in the direction the search goes.
    FindNext,
    /// The next match the other way.
    FindPrev,
    /// Remember this place in the view history, or save it as bookmark `n`.
    Mark,
    /// Previous view, or go to bookmark `n`.
    Back,
    Forward,
    FitWidth,
    FitHeight,
    FitPage,
    /// Zoom to `n` percent.
    ZoomTo,
    ZoomIn,
    ZoomOut,
    RotateLeft,
    RotateRight,
    Invert,
}

impl Kind {
    /// Every action, in the order Customize keys lists them.
    pub const ALL: [Kind; 26] = [
        Kind::ScrollLeft,
        Kind::ScrollRight,
        Kind::ScrollDown,
        Kind::ScrollUp,
        Kind::ScreenForward,
        Kind::ScreenBack,
        Kind::PrevPage,
        Kind::NextPage,
        Kind::GoFirst,
        Kind::GoLast,
        Kind::Mark,
        Kind::Back,
        Kind::Forward,
        Kind::SearchForward,
        Kind::SearchBackward,
        Kind::FindNext,
        Kind::FindPrev,
        Kind::FitWidth,
        Kind::FitHeight,
        Kind::FitPage,
        Kind::ZoomTo,
        Kind::ZoomIn,
        Kind::ZoomOut,
        Kind::RotateLeft,
        Kind::RotateRight,
        Kind::Invert,
    ];

    /// A stable name for saved settings.
    pub fn id(self) -> &'static str {
        match self {
            Self::ScrollLeft => "scroll_left",
            Self::ScrollRight => "scroll_right",
            Self::ScrollDown => "scroll_down",
            Self::ScrollUp => "scroll_up",
            Self::ScreenForward => "screen_forward",
            Self::ScreenBack => "screen_back",
            Self::PrevPage => "prev_page",
            Self::NextPage => "next_page",
            Self::GoFirst => "go_first",
            Self::GoLast => "go_last",
            Self::SearchForward => "search_forward",
            Self::SearchBackward => "search_backward",
            Self::FindNext => "find_next",
            Self::FindPrev => "find_prev",
            Self::Mark => "mark",
            Self::Back => "back",
            Self::Forward => "forward",
            Self::FitWidth => "fit_width",
            Self::FitHeight => "fit_height",
            Self::FitPage => "fit_page",
            Self::ZoomTo => "zoom_to",
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::RotateLeft => "rotate_left",
            Self::RotateRight => "rotate_right",
            Self::Invert => "invert",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// What the action does, in English (translated where it is shown).
    pub fn label(self) -> &'static str {
        match self {
            Self::ScrollLeft => "Scroll left",
            Self::ScrollRight => "Scroll right",
            Self::ScrollDown => "Scroll down",
            Self::ScrollUp => "Scroll up",
            Self::ScreenForward => "Forward one screen",
            Self::ScreenBack => "Back one screen",
            Self::PrevPage => "Previous page",
            Self::NextPage => "Next page",
            Self::GoFirst => "First page (12g: page 12)",
            Self::GoLast => "Last page (12G: page 12)",
            Self::SearchForward => "Search forward",
            Self::SearchBackward => "Search backward",
            Self::FindNext => "Next match",
            Self::FindPrev => "Previous match",
            Self::Mark => "Keep this place (7m: bookmark 7)",
            Self::Back => "Previous view (7t: bookmark 7)",
            Self::Forward => "Next view",
            Self::FitWidth => "Fit width",
            Self::FitHeight => "Fit height",
            Self::FitPage => "Fit page to the window",
            Self::ZoomTo => "Zoom to a percentage (150z)",
            Self::ZoomIn => "Zoom in",
            Self::ZoomOut => "Zoom out",
            Self::RotateLeft => "Rotate counter-clockwise",
            Self::RotateRight => "Rotate clockwise",
            Self::Invert => "Invert colours",
        }
    }

    /// The heading the shortcuts window lists the action under.
    pub fn group(self) -> &'static str {
        match self {
            Self::ScrollLeft | Self::ScrollRight | Self::ScrollDown | Self::ScrollUp | Self::ScreenForward | Self::ScreenBack => "Scroll",
            Self::PrevPage | Self::NextPage | Self::GoFirst | Self::GoLast | Self::Mark | Self::Back | Self::Forward => "Go to",
            Self::SearchForward | Self::SearchBackward | Self::FindNext | Self::FindPrev => "Search",
            _ => "View",
        }
    }

    /// The keys that do it until it is given another: the first is the one shown first.
    pub fn defaults(self) -> &'static [char] {
        match self {
            Self::ScrollLeft => &['h'],
            Self::ScrollRight => &['l'],
            Self::ScrollDown => &['j'],
            Self::ScrollUp => &['k'],
            Self::ScreenForward => &[' '],
            Self::ScreenBack => &['b'],
            Self::PrevPage => &[','],
            Self::NextPage => &['.'],
            Self::GoFirst => &['g'],
            Self::GoLast => &['G'],
            Self::SearchForward => &['/'],
            Self::SearchBackward => &['?'],
            Self::FindNext => &['n'],
            Self::FindPrev => &['N'],
            Self::Mark => &['m'],
            Self::Back => &['t'],
            Self::Forward => &['T'],
            Self::FitWidth => &['W'],
            Self::FitHeight => &['H'],
            // F fits the page to the window; Z is the same, as in other viewers.
            Self::FitPage => &['F', 'f', 'Z'],
            Self::ZoomTo => &['z'],
            // `=` is the unshifted key of `+` on most layouts.
            Self::ZoomIn => &['+', '='],
            Self::ZoomOut => &['-'],
            Self::RotateLeft => &['['],
            Self::RotateRight => &[']'],
            Self::Invert => &['I'],
        }
    }
}

/// Why a key could not be given to an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindError {
    /// A digit (they type a number before a key), a space-like or control character.
    Unusable,
    /// A key a tool uses (Select, Hand, sticky note, highlight, draw).
    Reserved,
    /// Another action has it.
    Taken(Kind),
}

/// How a key is shown: Space by name, anything else as it is.
pub fn key_name(c: char) -> String {
    if c == ' ' { "Space".to_owned() } else { c.to_string() }
}

/// The keys of the extra shortcuts: each action's own choice, or its default keys. Customize keys
/// changes it, and it is saved with the other settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Bindings {
    custom: BTreeMap<Kind, char>,
}

impl Bindings {
    /// The action typing `c` stands for, if any. A key given to an action wins over the default
    /// of another.
    pub fn kind_of(&self, c: char) -> Option<Kind> {
        if let Some((kind, _)) = self.custom.iter().find(|(_, key)| **key == c) {
            return Some(*kind);
        }
        Kind::ALL.into_iter().find(|kind| !self.custom.contains_key(kind) && kind.defaults().contains(&c))
    }

    /// The keys that do `kind` now.
    pub fn keys(&self, kind: Kind) -> Vec<char> {
        match self.custom.get(&kind) {
            Some(key) => vec![*key],
            None => kind.defaults().iter().copied().filter(|c| self.kind_of(*c) == Some(kind)).collect(),
        }
    }

    /// The keys that do `kind`, as shown: `F / f / Z`.
    pub fn label(&self, kind: Kind) -> String {
        self.keys(kind).into_iter().map(key_name).collect::<Vec<_>>().join(" / ")
    }

    /// Whether `kind` has a key of the user's choosing.
    pub fn is_custom(&self, kind: Kind) -> bool {
        self.custom.contains_key(&kind)
    }

    /// Whether every action keeps its default keys.
    pub fn is_default(&self) -> bool {
        self.custom.is_empty()
    }

    /// Give `kind` the key `c`, instead of its default keys.
    pub fn set(&mut self, kind: Kind, c: char) -> Result<(), BindError> {
        if c.is_control() || c.is_ascii_digit() || (c.is_whitespace() && c != ' ') {
            return Err(BindError::Unusable);
        }
        if RESERVED.contains(&c.to_ascii_lowercase()) {
            return Err(BindError::Reserved);
        }
        if let Some(other) = self.kind_of(c)
            && other != kind
        {
            return Err(BindError::Taken(other));
        }
        if kind.defaults().contains(&c) {
            self.custom.remove(&kind);
        } else {
            self.custom.insert(kind, c);
        }
        Ok(())
    }

    /// Give `kind` its default keys again.
    pub fn reset(&mut self, kind: Kind) {
        self.custom.remove(&kind);
    }

    pub fn reset_all(&mut self) {
        self.custom.clear();
    }

    /// For the saved settings: the actions that were changed, by id.
    pub fn to_json(&self) -> Value {
        Value::Object(self.custom.iter().map(|(kind, c)| (kind.id().to_owned(), Value::String(c.to_string()))).collect())
    }

    /// From saved settings, which are untrusted: unknown actions, keys that are not one usable
    /// character and clashes are skipped one by one, and the rest are kept.
    pub fn from_json(value: &Value) -> Self {
        let mut bindings = Self::default();
        let Some(map) = value.as_object() else { return bindings };
        for (id, key) in map.iter().take(Kind::ALL.len() * 2) {
            let (Some(kind), Some(key)) = (Kind::from_id(id), key.as_str()) else { continue };
            let mut chars = key.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                let _ = bindings.set(kind, c);
            }
        }
        bindings
    }
}

/// A key with the number typed before it, if any.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Command {
    pub kind: Kind,
    pub count: Option<u32>,
}

/// What typing one character came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Feed {
    /// Not one of these keys: it stays for whatever reads text next.
    Ignored,
    /// A digit of a number being typed.
    Pending,
    /// A key to run.
    Run(Command),
}

/// The state of the extra keys for one document: the number being typed and its numbered
/// bookmarks.
#[derive(Clone, Debug, Default)]
pub(crate) struct Keys {
    count: Option<u32>,
    /// When the last digit was typed, in egui time.
    at: f64,
    marks: BTreeMap<u32, usize>,
}

impl Keys {
    /// Take one typed character at time `now` (egui's, in seconds).
    pub(crate) fn feed(&mut self, c: char, now: f64, bindings: &Bindings) -> Feed {
        // A number left standing for a while is forgotten; a clock that is not a number too.
        let fresh = (now - self.at).abs() <= COUNT_SECONDS;
        if self.count.is_some() && !fresh {
            self.count = None;
        }
        if let Some(digit) = c.to_digit(10) {
            // A leading 0 is not a number (and is not a key here either).
            if digit == 0 && self.count.is_none() {
                return Feed::Ignored;
            }
            let n = self.count.unwrap_or(0).saturating_mul(10).saturating_add(digit).min(MAX_COUNT);
            self.count = Some(n);
            self.at = now;
            return Feed::Pending;
        }
        let count = self.count.take();
        match bindings.kind_of(c) {
            Some(kind) => Feed::Run(Command { kind, count }),
            None => Feed::Ignored,
        }
    }

    /// Save `page` as bookmark `n`. A new bookmark past the limit is refused; an old one is moved.
    fn save_mark(&mut self, n: u32, page: usize) {
        if self.marks.contains_key(&n) || self.marks.len() < MAX_MARKS {
            self.marks.insert(n, page);
        }
    }

    /// The page saved as bookmark `n`.
    fn mark(&self, n: u32) -> Option<usize> {
        self.marks.get(&n).copied()
    }

    /// Do `command` to `view`.
    fn run(&mut self, view: &mut DocView, command: Command, speed: ScrollSpeed) {
        let line = KEY_SCROLL_LINE * speed.factor();
        // A number typed for page keys counts pages from 1.
        let page = |n: u32| usize::try_from(n.saturating_sub(1)).unwrap_or(usize::MAX);
        match command.kind {
            Kind::ScrollLeft => view.key_side -= line,
            Kind::ScrollRight => view.key_side += line,
            Kind::ScrollDown => step_vertical(view, true, line),
            Kind::ScrollUp => step_vertical(view, false, -line),
            Kind::ScreenForward | Kind::ScreenBack => {
                let screen = (view.viewport_h - KEY_SCROLL_LINE).max(KEY_SCROLL_LINE);
                let forward = command.kind == Kind::ScreenForward;
                step_vertical(view, forward, if forward { screen } else { -screen });
            }
            Kind::PrevPage => view.step_page(false),
            Kind::NextPage => view.step_page(true),
            Kind::GoFirst => view.go_to_page(command.count.map_or(0, page)),
            Kind::GoLast => match command.count {
                Some(n) => view.go_to_page(page(n)),
                None => view.go_to_end(),
            },
            Kind::SearchForward => view.open_find_bottom(false),
            Kind::SearchBackward => view.open_find_bottom(true),
            // Both follow the direction of the search that is open: n goes on, N turns round.
            Kind::FindNext => view.find_step(!view.find_backward()),
            Kind::FindPrev => view.find_step(view.find_backward()),
            Kind::Mark => match command.count {
                Some(n) => self.save_mark(n, view.current),
                None => {
                    if view.back.last() != Some(&view.current) {
                        view.back.push(view.current);
                        if view.back.len() > MAX_HISTORY {
                            view.back.remove(0);
                        }
                    }
                }
            },
            Kind::Back => match command.count {
                Some(n) => {
                    if let Some(mark) = self.mark(n) {
                        view.go_to_page(mark);
                    }
                }
                None => {
                    view.view_history(false);
                }
            },
            Kind::Forward => {
                view.view_history(true);
            }
            Kind::FitWidth => view.set_fit(Fit::Width),
            Kind::FitHeight => view.set_fit(Fit::Height),
            Kind::FitPage => view.set_fit(Fit::Page),
            Kind::ZoomTo => {
                if let Some(percent) = command.count.filter(|p| *p > 0) {
                    // At most MAX_COUNT percent: an exact f32.
                    view.set_zoom(percent as f32 / 100.0);
                }
            }
            Kind::ZoomIn => view.zoom_step(true),
            Kind::ZoomOut => view.zoom_step(false),
            Kind::RotateLeft => view.rotate_view(false),
            Kind::RotateRight => view.rotate_view(true),
            Kind::Invert => view.pending_action = Some(ViewAction::ToggleInvertColors),
        }
    }
}

/// Run this frame's extra keys on `view`. Not while the page grid is up, or a page text editor,
/// draft or form field has the keyboard (a text field that has it makes the caller skip this).
pub(crate) fn handle(view: &mut DocView, ctx: &egui::Context, keys: PageKeys, speed: ScrollSpeed, bindings: &Bindings) {
    if view.organize || view.line_editor.is_some() || view.content.draft.is_some() || view.forms.focus.is_some() {
        return;
    }
    let now = ctx.input(|i| i.time);
    // Space also presses a button or box that has the keyboard focus: that press is theirs.
    let focused = ctx.memory(|m| m.focused().is_some());
    // Full screen already turns pages with Space.
    let presenting = view.presentation.is_some();
    // The state is part of the view the keys act on.
    let mut state = std::mem::take(&mut view.extra_keys);
    let mut commands = Vec::new();
    keys.take_text(ctx, |c| match state.feed(c, now, bindings) {
        _ if c == ' ' && (focused || presenting) => false,
        Feed::Ignored => false,
        Feed::Pending => true,
        Feed::Run(command) => {
            commands.push(command);
            true
        }
    });
    for command in commands {
        state.run(view, command, speed);
    }
    view.extra_keys = state;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(keys: &mut Keys, typed: &str) -> Vec<Feed> {
        let bindings = Bindings::default();
        typed.chars().map(|c| keys.feed(c, 0.0, &bindings)).collect()
    }

    fn cmd(kind: Kind, count: Option<u32>) -> Feed {
        Feed::Run(Command { kind, count })
    }

    #[test]
    fn a_key_runs_and_an_unknown_one_stays_for_others() {
        let mut keys = Keys::default();
        assert_eq!(run(&mut keys, "jx"), [cmd(Kind::ScrollDown, None), Feed::Ignored]);
        let b = Bindings::default();
        assert_eq!(b.kind_of('q'), None);
        assert_eq!(b.kind_of(' '), Some(Kind::ScreenForward));
        assert_eq!(b.kind_of('p'), None, "p is the Hand tool's, which is not a page key");
    }

    #[test]
    fn f_fits_the_page_and_so_do_its_aliases() {
        let b = Bindings::default();
        for c in ['F', 'f', 'Z'] {
            assert_eq!(b.kind_of(c), Some(Kind::FitPage), "{c}");
        }
        assert_eq!(b.label(Kind::FitPage), "F / f / Z");
    }

    #[test]
    fn slash_and_question_mark_are_different_searches() {
        let b = Bindings::default();
        assert_eq!(b.kind_of('/'), Some(Kind::SearchForward));
        assert_eq!(b.kind_of('?'), Some(Kind::SearchBackward));
        assert_eq!(b.kind_of('n'), Some(Kind::FindNext));
        assert_eq!(b.kind_of('N'), Some(Kind::FindPrev));
    }

    #[test]
    fn a_number_applies_to_the_next_key_only() {
        let mut keys = Keys::default();
        assert_eq!(run(&mut keys, "12gg"), [Feed::Pending, Feed::Pending, cmd(Kind::GoFirst, Some(12)), cmd(Kind::GoFirst, None)]);
        assert_eq!(run(&mut keys, "150z"), [Feed::Pending, Feed::Pending, Feed::Pending, cmd(Kind::ZoomTo, Some(150))]);
    }

    #[test]
    fn a_leading_zero_is_not_a_number_but_a_later_zero_is() {
        let mut keys = Keys::default();
        assert_eq!(run(&mut keys, "0"), [Feed::Ignored], "left for whatever else wants it");
        assert_eq!(run(&mut keys, "10G"), [Feed::Pending, Feed::Pending, cmd(Kind::GoLast, Some(10))]);
    }

    #[test]
    fn a_character_that_is_not_a_key_drops_the_number() {
        let mut keys = Keys::default();
        assert_eq!(run(&mut keys, "5xg"), [Feed::Pending, Feed::Ignored, cmd(Kind::GoFirst, None)]);
    }

    #[test]
    fn a_number_left_standing_is_forgotten() {
        let b = Bindings::default();
        let mut keys = Keys::default();
        assert_eq!(keys.feed('7', 10.0, &b), Feed::Pending);
        assert_eq!(keys.feed('g', 10.0 + COUNT_SECONDS - 0.5, &b), cmd(Kind::GoFirst, Some(7)));
        assert_eq!(keys.feed('7', 20.0, &b), Feed::Pending);
        assert_eq!(keys.feed('g', 20.0 + COUNT_SECONDS + 0.5, &b), cmd(Kind::GoFirst, None), "too long idle");
        assert_eq!(keys.feed('7', 30.0, &b), Feed::Pending);
        assert_eq!(keys.feed('g', f64::NAN, &b), cmd(Kind::GoFirst, None), "a broken clock forgets it too");
    }

    #[test]
    fn a_long_number_stops_at_the_largest() {
        let mut keys = Keys::default();
        let typed: String = "9".repeat(40) + "g";
        assert_eq!(run(&mut keys, &typed).last().copied(), Some(cmd(Kind::GoFirst, Some(MAX_COUNT))));
    }

    #[test]
    fn the_shifted_and_unshifted_keys_are_different_keys() {
        let b = Bindings::default();
        for (a, c) in [('g', 'G'), ('n', 'N'), ('t', 'T'), ('h', 'H')] {
            assert_ne!(b.kind_of(a), b.kind_of(c), "{a} and {c}");
        }
        assert_eq!(b.kind_of('+'), b.kind_of('='));
    }

    #[test]
    fn numbered_bookmarks_are_bounded() {
        let mut keys = Keys::default();
        for n in 1..=(MAX_MARKS as u32 + 10) {
            keys.save_mark(n, 3);
        }
        assert_eq!(keys.marks.len(), MAX_MARKS);
        assert_eq!(keys.mark(1), Some(3));
        assert_eq!(keys.mark(MAX_MARKS as u32 + 1), None, "a new one past the limit is refused");
        keys.save_mark(1, 9);
        assert_eq!(keys.mark(1), Some(9), "an old one still moves");
    }

    #[test]
    fn a_chosen_key_replaces_the_default_and_frees_it() {
        let mut b = Bindings::default();
        b.set(Kind::ScrollDown, 'x').unwrap();
        assert_eq!(b.kind_of('x'), Some(Kind::ScrollDown));
        assert_eq!(b.kind_of('j'), None, "its old key does nothing now");
        assert_eq!(b.label(Kind::ScrollDown), "x");
        assert!(b.is_custom(Kind::ScrollDown) && !b.is_default());
        // A freed key can be given to another action.
        b.set(Kind::ScrollUp, 'j').unwrap();
        assert_eq!(b.kind_of('j'), Some(Kind::ScrollUp));
        // Choosing a default again forgets the choice.
        assert_eq!(b.set(Kind::ScrollDown, 'j'), Err(BindError::Taken(Kind::ScrollUp)));
        b.reset(Kind::ScrollUp);
        b.set(Kind::ScrollDown, 'j').unwrap();
        assert!(!b.is_custom(Kind::ScrollDown));
        b.reset_all();
        assert!(b.is_default());
    }

    #[test]
    fn a_key_in_use_or_unusable_is_refused() {
        let mut b = Bindings::default();
        assert_eq!(b.set(Kind::ScrollDown, 'k'), Err(BindError::Taken(Kind::ScrollUp)));
        assert_eq!(b.set(Kind::ScrollDown, '5'), Err(BindError::Unusable), "digits type a number");
        assert_eq!(b.set(Kind::ScrollDown, '\t'), Err(BindError::Unusable));
        assert_eq!(b.set(Kind::ScrollDown, 'p'), Err(BindError::Reserved), "the Hand tool's");
        assert_eq!(b.set(Kind::ScrollDown, 'V'), Err(BindError::Reserved), "and the Select tool's, shifted or not");
        for tool_key in ['s', 'S', 'u', 'U', 'd', 'D'] {
            assert_eq!(b.set(Kind::ScrollDown, tool_key), Err(BindError::Reserved), "{tool_key}: the comment tools'");
        }
        assert!(b.is_default(), "a refusal changes nothing");
        // Space is allowed once it is free.
        assert_eq!(b.set(Kind::ScrollDown, ' '), Err(BindError::Taken(Kind::ScreenForward)));
        b.set(Kind::ScreenForward, 'x').unwrap();
        b.set(Kind::ScrollDown, ' ').unwrap();
        assert_eq!(b.label(Kind::ScrollDown), "Space");
    }

    #[test]
    fn choices_are_saved_and_untrusted_settings_are_filtered() {
        let mut b = Bindings::default();
        b.set(Kind::FitPage, 'x').unwrap();
        b.set(Kind::Invert, '*').unwrap();
        assert_eq!(Bindings::from_json(&b.to_json()), b);
        let hostile = serde_json::json!({
            "fit_page": "ab", "scroll_down": "", "no_such_action": "q", "zoom_in": 5, "invert": "5",
            "scroll_left": "k", "scroll_up": "k", "mark": "\u{7}", "find_next": "~"
        });
        let got = Bindings::from_json(&hostile);
        assert_eq!(got.kind_of('~'), Some(Kind::FindNext), "the good entry survives");
        assert!(!got.is_custom(Kind::FitPage) && !got.is_custom(Kind::ScrollDown) && !got.is_custom(Kind::Invert));
        assert!(!got.is_custom(Kind::ZoomIn) && !got.is_custom(Kind::Mark));
        assert_eq!(Bindings::from_json(&serde_json::json!("nonsense")), Bindings::default());
    }

    #[test]
    fn every_action_has_a_name_a_key_and_a_group() {
        let b = Bindings::default();
        let mut seen = std::collections::BTreeSet::new();
        for kind in Kind::ALL {
            assert_eq!(Kind::from_id(kind.id()), Some(kind), "{kind:?}");
            assert!(!kind.label().is_empty() && !kind.group().is_empty());
            assert!(!b.keys(kind).is_empty(), "{kind:?} has a key");
            for c in kind.defaults() {
                assert!(seen.insert(*c), "{c} is the default of two actions");
            }
        }
    }
}
