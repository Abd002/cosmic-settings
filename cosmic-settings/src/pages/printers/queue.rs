use cosmic::app::Task;
use cosmic_settings_page as page;

#[derive(Clone, Debug)]
pub enum Message {
    LoadPrinter { printer_name: String },
}

impl From<Message> for crate::pages::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::PrinterQueue(message)
    }
}

impl From<Message> for crate::app::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::PrinterQueue(message).into()
    }
}

#[derive(Default)]
pub struct Page {
    entity: page::Entity,
    printer_name: Option<String>,
}

impl page::AutoBind<crate::pages::Message> for Page {}

impl page::Page<crate::pages::Message> for Page {
    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn info(&self) -> page::Info {
        page::Info::new("printer-queue", "printer-symbolic")
            .title(fl!("printer-queue"))
            .description(fl!("printer-queue-description"))
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::LoadPrinter { printer_name } => {
                self.printer_name = Some(printer_name);
            }
        }

        Task::none()
    }
}
