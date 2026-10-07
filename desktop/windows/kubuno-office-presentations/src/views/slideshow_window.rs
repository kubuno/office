//! Code-behind of the slideshow window (`slideshow_window.kbview`): the stage over the whole screen, modal over
//! the editor; it closes on Escape or ✕ and the editor shows the slide the show ended on.

use std::rc::Rc;

use kubuno_desktop::prelude::*;
use kubuno_desktop::views::component::Control as _;

use crate::controls::slide_show::{ShowClosedEventArgs, SlideShow};
use crate::model::Shared;
use crate::platform::d2d::Fonts;

#[kubuno_desktop::view("slideshow_window.kbview")]
pub struct SlideshowWindow {
    #[control]
    stage: Custom<SlideShow>,
    /// The deck index of the slide shown last.
    pub last_slide: usize,
    /// What to show, handed to the stage once the window is open (its control exists then).
    pending: Option<(Shared, usize)>,
}

impl SlideshowWindow {
    pub fn new(doc: Shared, start: usize) -> Self {
        let mut w = Self { pending: Some((doc, start)), ..Self::default() };
        w.initialize_component();
        w
    }

    fn slideshow_load(&mut self) {
        if let Some((doc, start)) = self.pending.take() {
            self.stage.with(|s| s.start(doc, start));
        }
        // The keys go to the stage once the window is active.
        if let Some(d) = self.dispatcher() {
            drop(d.begin_invoke(|w: &mut SlideshowWindow| {
                w.stage.with(|s| s.focus());
            }));
        }
    }

    /// Shows the slideshow from deck slide `start`, over `owner`; returns the slide it ended on.
    pub fn present(owner: &dyn AsForm, doc: Shared, start: usize, _fonts: Option<Rc<Fonts>>) -> usize {
        let mut w = Self::new(doc, start);
        // Opened at the size and place of the monitor: a window resized after it opens is not laid out again here.
        if let Some((x, y, cw, ch)) = owner.as_form().handle().and_then(crate::platform::fullscreen::monitor_dip) {
            let form = w.form();
            form.set_start_position(StartPosition::Manual);
            form.root().set_property("X", x);
            form.root().set_property("Y", y);
            form.set_client_size(cw, ch);
        }
        w.show_dialog(owner);
        w.last_slide
    }

    fn slideshow_key(&mut self, e: &mut KeyEventArgs) {
        if self.stage.with(|s| s.key(e.key.0)) == Some(true) {
            e.handled = true;
        }
    }

    fn stage_closed(&mut self, e: &ShowClosedEventArgs) {
        self.last_slide = e.slide;
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
