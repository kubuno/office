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
}

impl SlideshowWindow {
    pub fn new(doc: Shared, start: usize) -> Self {
        let mut w = Self::default();
        w.initialize_component();
        w.stage.with(|s| {
            s.start(doc, start);
            s.focus();
        });
        w
    }

    /// Shows the slideshow from deck slide `start`, over `owner`; returns the slide it ended on.
    pub fn present(owner: &dyn AsForm, doc: Shared, start: usize, _fonts: Option<Rc<Fonts>>) -> usize {
        let mut w = Self::new(doc, start);
        w.show_dialog(owner);
        w.last_slide
    }

    fn stage_closed(&mut self, e: &ShowClosedEventArgs) {
        self.last_slide = e.slide;
        self.set_dialog_result(DialogResult::Ok);
        self.close();
    }
}
