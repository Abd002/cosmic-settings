use cosmic::{Apply, Element, widget};

use super::Message;

pub fn dialog(close: Message) -> Element<'static, crate::pages::Message> {
    widget::dialog()
        .title(fl!("add-printer"))
        .secondary_action(widget::button::standard(fl!("cancel")).on_press(close.into()))
        .apply(Element::from)
}
