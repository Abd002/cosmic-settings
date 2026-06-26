use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, column, dropdown, icon, row, settings, space::horizontal as horizontal_space, text,
};
use cosmic_settings_page::{self as page, Section, section};
use slotmap::SlotMap;

use super::PrinterEntry;

#[derive(Clone, Debug)]
pub enum Message {
    EditLocation(String),
    LoadPrinter {
        printer: PrinterEntry,
        is_default: bool,
    },
    OpenPrinterQueue(String),
    RemovePrinter(String),
    SelectPaperSize(String, usize),
    SelectPrintSides(String, usize),
    ToggleDefaultPrinter(String, bool),
    Surface(cosmic::surface::Action),
}

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

pub struct Page {
    entity: page::Entity,
    queue_page: page::Entity,
    printer: Option<PrinterEntry>,
    is_default: bool,
    paper_size_labels: Vec<String>,
    print_sides_labels: Vec<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            queue_page: page::Entity::default(),
            printer: None,
            is_default: false,
            paper_size_labels: vec![fl!("letter-paper-size"), "A4".into(), "Legal".into()],
            print_sides_labels: vec![fl!("print-one-side"), fl!("print-both-sides")],
        }
    }
}

impl page::AutoBind<crate::pages::Message> for Page {
    fn sub_pages(
        mut page: page::Insert<crate::pages::Message>,
    ) -> page::Insert<crate::pages::Message> {
        let queue_id = page.sub_page_with_id::<super::queue::Page>();

        if let Some(model) = page.model.page_mut::<Page>() {
            model.queue_page = queue_id;
        }

        page
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

    fn content(
        &self,
        sections: &mut SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        crate::slab!(descriptions {
            open_queue_label = fl!("open-printer-queue");
        });

        Some(vec![sections.insert(
            Section::default().descriptions(descriptions).view::<Page>(
                move |_binder, page, section| {
                    let Some(printer) = page.printer.as_ref() else {
                        return empty_state().into();
                    };

                    view_details(
                        page,
                        printer,
                        page.is_default,
                        &section.descriptions[open_queue_label],
                    )
                },
            ),
        )])
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::LoadPrinter {
                printer,
                is_default,
            } => {
                self.printer = Some(printer);
                self.is_default = is_default;
            }
            Message::ToggleDefaultPrinter(_, _)
            | Message::SelectPaperSize(_, _)
            | Message::SelectPrintSides(_, _)
            | Message::RemovePrinter(_)
            | Message::EditLocation(_) => {}
            Message::OpenPrinterQueue(id) => {
                if let Some(printer) = self.printer.as_ref().filter(|printer| printer.id == id) {
                    return Task::batch([
                        cosmic::task::message(crate::app::Message::PageMessage(
                            crate::pages::Message::PrinterQueue(
                                super::queue::Message::LoadPrinter {
                                    printer_name: printer.name.clone(),
                                },
                            ),
                        )),
                        cosmic::task::message(crate::app::Message::PageMessage(
                            crate::pages::Message::Page(self.queue_page),
                        )),
                    ]);
                }
            }
            Message::Surface(action) => {
                return cosmic::task::message(crate::app::Message::Surface(action));
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

fn view_details<'a>(
    page: &'a Page,
    printer: &'a PrinterEntry,
    is_default: bool,
    open_queue: &'a str,
) -> Element<'a, crate::pages::Message> {
    let spacing = cosmic::theme::spacing();
    let mut content = column::with_capacity(10).spacing(spacing.space_m);

    content = content.push(settings::section().add(
        settings::item::builder(fl!("default-printer")).toggler(is_default, {
            let id = printer.id.clone();
            move |value| Message::ToggleDefaultPrinter(id.clone(), value).into()
        }),
    ));

    let mut info_section = settings::section().title(fl!("printer-information"));

    info_section = info_section.add(crate::widget::go_next_with_item(
        open_queue,
        text::body(&printer.queue_status),
        Message::OpenPrinterQueue(printer.id.clone()),
    ));

    info_section = info_section.add(
        settings::item::builder(fl!("location")).control(
            row::with_capacity(2)
                .push(text::body(&printer.location))
                .push(
                    button::icon(icon::from_name("edit-symbolic"))
                        .extra_small()
                        .on_press(Message::EditLocation(printer.id.clone()).into()),
                )
                .align_y(Alignment::Center)
                .spacing(spacing.space_s),
        ),
    );

    info_section = info_section.add(settings::item(fl!("model"), text::body(&printer.model)));
    info_section = info_section.add(settings::item(
        fl!("device-name"),
        text::body(&printer.name),
    ));
    info_section = info_section.add(settings::item(
        fl!("driver-version"),
        text::body(&printer.driver_version),
    ));

    content = content.push(info_section);

    let mut preferences = settings::section().title(fl!("printing-preferences"));

    preferences = preferences.add(settings::item::builder(fl!("paper-size")).control(
        dropdown::popup_dropdown(
            &page.paper_size_labels,
            Some(printer.paper_size_idx),
            {
                let id = printer.id.clone();
                move |idx| Message::SelectPaperSize(id.clone(), idx)
            },
            cosmic::iced::window::Id::RESERVED,
            Message::Surface,
            |action| {
                crate::app::Message::PageMessage(crate::pages::Message::PrinterDetails(action))
            },
        ),
    ));

    preferences = preferences.add(settings::item::builder(fl!("print-sides")).control(
        dropdown::popup_dropdown(
            &page.print_sides_labels,
            Some(printer.print_sides_idx),
            {
                let id = printer.id.clone();
                move |idx| Message::SelectPrintSides(id.clone(), idx)
            },
            cosmic::iced::window::Id::RESERVED,
            Message::Surface,
            |action| {
                crate::app::Message::PageMessage(crate::pages::Message::PrinterDetails(action))
            },
        ),
    ));

    content = content.push(preferences);

    if !printer.supplies.is_empty() {
        let mut supplies = settings::section().title(fl!("supplies"));

        for supply in &printer.supplies {
            supplies = supplies.add(settings::item(
                &*supply.name,
                text::body(format!("{}%", supply.level_percent)),
            ));
        }

        content = content.push(supplies);
    }

    content = content.push(
        row::with_capacity(2)
            .push(horizontal_space())
            .push(
                button::destructive(fl!("remove-printer"))
                    .on_press(Message::RemovePrinter(printer.id.clone()).into()),
            )
            .align_y(Alignment::Center),
    );

    Element::from(content.width(Length::Fill)).map(crate::pages::Message::PrinterDetails)
}
