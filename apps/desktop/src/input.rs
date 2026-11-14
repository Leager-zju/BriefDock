//! A held arrow has one outcome: step once, or restore the temporary rate.
use super::*;
use std::time::{Duration, Instant};
const DELAY: Duration = Duration::from_millis(400);
#[derive(Default)]
pub struct Hold {
    pressed: Option<Instant>,
    consumed: bool,
    boosted: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Released {
    None,
    Step,
    Restore,
}
impl Hold {
    pub fn press(&mut self, now: Instant) {
        if self.pressed.is_none() {
            self.pressed = Some(now);
            self.consumed = false;
            self.boosted = false;
        }
    }
    pub fn pending(&self) -> bool {
        self.pressed.is_some() && !self.consumed
    }
    pub fn tick(&mut self, now: Instant, playing: bool) -> bool {
        if self.pending() && now.duration_since(self.pressed.unwrap()) >= DELAY {
            self.consumed = true;
            self.boosted = playing;
            return playing;
        }
        false
    }
    pub fn release(&mut self, now: Instant) -> Released {
        let outcome = if self.boosted {
            Released::Restore
        } else if self
            .pressed
            .is_some_and(|pressed| now.duration_since(pressed) < DELAY)
            && !self.consumed
        {
            Released::Step
        } else {
            Released::None
        };
        self.cancel();
        outcome
    }
    pub fn cancel(&mut self) -> bool {
        self.pressed = None;
        self.consumed = false;
        std::mem::take(&mut self.boosted)
    }
}
impl App {
    pub(super) fn keyboard(
        &mut self,
        event: iced::keyboard::Event,
        status: iced::event::Status,
    ) -> Task<Message> {
        use iced::keyboard::{Event, Key, key::Named};
        let now = Instant::now();
        if let Event::KeyReleased {
            key: Key::Named(Named::ArrowRight),
            ..
        } = event
        {
            return match self.hold.release(now) {
                Released::Restore => self.request("media.boost".into(), json!({"value":false})),
                Released::Step => self.step(5.),
                Released::None => Task::none(),
            };
        }
        if status == iced::event::Status::Captured
            || self.shortcut_recording
            || self.hidden
            || self.capsule
            || self.section != Section::Content
            || !self.page().is_some_and(|page| page.player_visible)
        {
            return Task::none();
        }
        if let Event::KeyPressed {
            key,
            repeat: false,
            modifiers,
            ..
        } = event
        {
            if modifiers.control() || modifiers.alt() || modifiers.logo() {
                return Task::none();
            }
            match key {
                Key::Named(Named::Tab) => {
                    if let Some(root) = &self.selected {
                        self.pages.get_mut(&root.id).unwrap().controls_keyboard = true;
                    }
                }
                Key::Named(Named::Space) => {
                    let playing = self
                        .page()
                        .is_some_and(|page| page.play_intent || page.media["playing"] == true);
                    return self.update(Message::Player(
                        if playing { "media.pause" } else { "media.play" },
                        json!({}),
                    ));
                }
                Key::Named(Named::ArrowLeft) => {
                    return self.step(-5.);
                }
                Key::Named(Named::ArrowRight)
                    if self.page().is_some_and(|page| page.media["loaded"] == true) =>
                {
                    self.hold.press(now)
                }
                _ => {}
            }
        }
        Task::none()
    }
    pub(super) fn step(&mut self, seconds: f64) -> Task<Message> {
        if self.hidden || self.capsule || self.section != Section::Content {
            return Task::none();
        }
        let Some(page) = self
            .page()
            .filter(|page| page.player_visible && page.media["loaded"] == true)
        else {
            return Task::none();
        };
        let position = (page.media["position"].as_f64().unwrap_or(0.) + seconds)
            .clamp(0., page.media["duration"].as_f64().unwrap_or(0.).max(0.));
        self.request("media.seek".into(), json!({"value":position}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_short_press_steps_once_and_repeats_do_not_restart_the_timer() {
        let now = Instant::now();
        let mut hold = Hold::default();
        hold.press(now);
        hold.press(now + Duration::from_millis(100));
        assert_eq!(
            hold.release(now + Duration::from_millis(300)),
            Released::Step
        );
        assert_eq!(
            hold.release(now + Duration::from_millis(350)),
            Released::None
        );
    }
    #[test]
    fn long_press_restores_once_and_paused_hold_never_starts_playback() {
        let now = Instant::now();
        let mut hold = Hold::default();
        hold.press(now);
        assert!(!hold.tick(now + Duration::from_millis(399), true));
        assert!(hold.tick(now + DELAY, true));
        assert!(!hold.pending());
        assert_eq!(hold.release(now + DELAY), Released::Restore);
        hold.press(now);
        assert!(!hold.tick(now + DELAY, false));
        assert_eq!(hold.release(now + DELAY), Released::None);
        hold.press(now);
        hold.tick(now + DELAY, true);
        assert!(hold.cancel());
        assert_eq!(hold.release(now + DELAY), Released::None);
    }
}
