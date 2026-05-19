use cosmic::Element;
use cosmic::app::Task;
use cosmic::widget::{column, settings, text};
use cosmic_settings_page::{self as page, Section, section};
use slotmap::SlotMap;

#[derive(Clone, Debug)]
pub enum Message {
    PauseJob(String),
    ResumeJob(String),
    CancelJob(String),
    ClearCompleted,
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

    fn content(
        &self,
        sections: &mut SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(Section::default().view::<Page>(
            |_binder, page, _section| {
                let Some(printer_name) = page.printer_name.as_deref() else {
                    return empty_state().into();
                };

                view_queue(printer_name)
            },
        ))])
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::PauseJob(_)
            | Message::ResumeJob(_)
            | Message::CancelJob(_)
            | Message::ClearCompleted => {}
            Message::LoadPrinter { printer_name } => {
                self.printer_name = Some(printer_name);
            }
        }

        Task::none()
    }
}

fn empty_state() -> Element<'static, crate::pages::Message> {
    column::with_capacity(1)
        .push(text::body(fl!("no-printer-selected")))
        .into()
}

fn view_queue(printer_name: &str) -> Element<'_, crate::pages::Message> {
    let spacing = cosmic::theme::spacing();

    Element::from(
        column::with_capacity(2)
            .spacing(spacing.space_m)
            .push(text::heading(printer_name))
            .push(settings::section().add(settings::item(
                fl!("printer-queue"),
                text::body(fl!("no-print-jobs")),
            ))),
    )
    .map(crate::pages::Message::PrinterQueue)
}
