use cosmic::app::Task;
use cosmic_settings_page as page;

#[derive(Clone, Debug)]
pub enum Message {}

impl From<Message> for crate::pages::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::PrinterDetails(message)
    }
}

impl From<Message> for crate::app::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::PrinterDetails(message).into()
    }
}

#[derive(Default)]
pub struct Page {
    entity: page::Entity,
}

impl page::AutoBind<crate::pages::Message> for Page {
    fn sub_pages(page: page::Insert<crate::pages::Message>) -> page::Insert<crate::pages::Message> {
        page.sub_page::<super::queue::Page>()
    }
}

impl page::Page<crate::pages::Message> for Page {
    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn info(&self) -> page::Info {
        page::Info::new("printer-details", "printer-symbolic")
            .title(fl!("printer-details"))
            .description(fl!("printer-details-description"))
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {}
    }
}
