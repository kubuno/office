//! What the window's controls share: the open presentation ([`Workspace`], desktop/common) and the image bytes.
//! One per window, handed to the slide list, the slide canvas and the slideshow.

use std::cell::RefCell;
use std::rc::Rc;

use kubuno_office_presentations_common::model::workspace::Workspace;

use crate::platform::images::ImageStore;

pub struct Doc {
    pub ws: RefCell<Workspace>,
    pub store: ImageStore,
}

pub type Shared = Rc<Doc>;

impl Doc {
    pub fn new(ws: Workspace, store: ImageStore) -> Shared {
        Rc::new(Doc { ws: RefCell::new(ws), store })
    }
}

/// Milliseconds of the framework's clock (the history coalescing and the caret blink).
pub fn now_ms() -> u64 {
    kubuno_desktop::controls::host::now_ms()
}
