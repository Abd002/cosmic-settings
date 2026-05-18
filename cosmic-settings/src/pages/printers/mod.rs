use cosmic::app::Task;
use cosmic_settings_page as page;

pub mod details;
pub mod queue;

#[derive(Default)]
pub struct Page {
    entity: page::Entity,
    details_page: page::Entity,
}

#[derive(Clone, Debug)]
pub enum Message {
    Refresh,
}

impl From<Message> for crate::pages::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::Printers(message)
    }
}

impl page::Page<crate::pages::Message> for Page {
    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn info(&self) -> page::Info {
        page::Info::new("printers", "printer-symbolic")
            .title(fl!("printers"))
            .description(fl!("printers-description"))
    }
}

impl page::AutoBind<crate::pages::Message> for Page {
    fn sub_pages(
        mut page: page::Insert<crate::pages::Message>,
    ) -> page::Insert<crate::pages::Message> {
        let details_id = page.sub_page_with_id::<details::Page>();

        if let Some(model) = page.model.page_mut::<Page>() {
            model.details_page = details_id;
        }

        page
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::Refresh => {}
        }
        Task::none()
    }
}
