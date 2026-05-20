use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::Alignment;
use cosmic::widget::{
    button, column, dropdown, row, settings, space::horizontal as horizontal_space, text,
};
use cosmic_settings_page as page;
use cosmic_settings_printers_client::{self as printers_client, CosmicPrintersProxy};
pub use cosmic_settings_printers_core::{PrinterEntry, PrinterStatus, SupplyLevel};
use slotmap::SlotMap;

pub mod add_printer;
pub mod details;
pub mod queue;

pub struct Page {
    entity: page::Entity,
    pub(crate) printers: Vec<PrinterEntry>,
    pub(crate) default_printer_id: Option<String>,
    pub(crate) show_add_printer_dialog: bool,
    details_page: page::Entity,
    default_printer_labels: Vec<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            printers: Vec::new(),
            default_printer_id: None,
            show_add_printer_dialog: false,
            details_page: page::Entity::default(),
            default_printer_labels: default_printer_labels(&[]),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    OpenAddPrinterDialog,
    CloseAddPrinterDialog,
    DefaultPrinterDropdown(usize),
    Refresh,
    PrintersLoaded(Result<Vec<PrinterEntry>, String>),
    SelectPrinter(PrinterEntry),
    Surface(cosmic::surface::Action),
}

impl From<Message> for crate::pages::Message {
    fn from(message: Message) -> Self {
        crate::pages::Message::Printers(message)
    }
}

impl From<Message> for crate::Message {
    fn from(message: Message) -> Self {
        crate::Message::PageMessage(message.into())
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

    fn dialog(&self) -> Option<Element<'_, crate::pages::Message>> {
        self.show_add_printer_dialog
            .then(|| add_printer::dialog(Message::CloseAddPrinterDialog))
    }

    fn on_enter(&mut self) -> cosmic::Task<crate::pages::Message> {
        cosmic::task::future(async {
            crate::pages::Message::Printers(Message::PrintersLoaded(load_printers().await))
        })
    }

    fn content(
        &self,
        sections: &mut SlotMap<page::section::Entity, page::Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(
            page::Section::default().view::<Page>(|_binder, page, _| view_list(page)),
        )])
    }
}

impl Page {
    fn default_printer_selection(&self) -> Option<usize> {
        match self.default_printer_id.as_deref() {
            Some(default_id) => self
                .printers
                .iter()
                .position(|printer| printer.id == default_id)
                .map(|idx| idx + 1)
                .or(Some(0)),
            None => Some(0),
        }
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
            Message::OpenAddPrinterDialog => {
                self.show_add_printer_dialog = true;
            }
            Message::CloseAddPrinterDialog => {
                self.show_add_printer_dialog = false;
            }
            Message::DefaultPrinterDropdown(idx) => {
                self.default_printer_id = idx
                    .checked_sub(1)
                    .and_then(|printer_idx| self.printers.get(printer_idx))
                    .map(|printer| printer.id.clone());
            }
            Message::Refresh => {
                return cosmic::task::future(async {
                    crate::Message::PageMessage(crate::pages::Message::Printers(
                        Message::PrintersLoaded(load_printers().await),
                    ))
                });
            }
            Message::PrintersLoaded(Ok(printers)) => {
                self.printers = printers;
                self.default_printer_labels = default_printer_labels(&self.printers);
            }
            Message::PrintersLoaded(Err(why)) => {
                tracing::error!(why, "failed to load printers");
                self.printers.clear();
                self.default_printer_id = None;
                self.default_printer_labels = default_printer_labels(&self.printers);
            }
            Message::SelectPrinter(printer) => {
                let is_default = self.default_printer_id.as_deref() == Some(printer.id.as_str());
                return Task::batch([
                    cosmic::task::message(crate::app::Message::PageMessage(
                        crate::pages::Message::PrinterDetails(details::Message::LoadPrinter {
                            printer,
                            is_default,
                        }),
                    )),
                    cosmic::task::message(crate::app::Message::PageMessage(
                        crate::pages::Message::Page(self.details_page),
                    )),
                ]);
            }
            Message::Surface(action) => {
                return cosmic::task::message(crate::app::Message::Surface(action));
            }
        }
        Task::none()
    }
}

async fn load_printers() -> Result<Vec<PrinterEntry>, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    let reply = client
        .conn
        .list_printers()
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))?;

    Ok(reply.printers)
}

fn printer_status_label(status: &PrinterStatus) -> String {
    match status {
        PrinterStatus::Ready => fl!("printer-ready"),
        PrinterStatus::Offline => fl!("printer-offline"),
        PrinterStatus::LowToner => fl!("printer-low-toner"),
    }
}

fn default_printer_labels(printers: &[PrinterEntry]) -> Vec<String> {
    std::iter::once(fl!("last-printer-used"))
        .chain(printers.iter().map(|printer| printer.name.clone()))
        .collect()
}

fn view_list(page: &Page) -> Element<'_, crate::pages::Message> {
    let spacing = cosmic::theme::spacing();

    let add_btn =
        button::standard(fl!("add-printer")).on_press(Message::OpenAddPrinterDialog.into());

    let header = row::with_capacity(2)
        .push(horizontal_space())
        .push(add_btn)
        .align_y(Alignment::Center);

    let default_section = settings::section().add(
        settings::item::builder(fl!("default-printer")).control(dropdown::popup_dropdown(
            &page.default_printer_labels,
            page.default_printer_selection(),
            Message::DefaultPrinterDropdown,
            cosmic::iced::window::Id::RESERVED,
            Message::Surface,
            |action| crate::app::Message::PageMessage(crate::pages::Message::Printers(action)),
        )),
    );

    let mut printers = settings::section().title(fl!("printers"));

    if page.printers.is_empty() {
        printers = printers.add(settings::item(
            fl!("no-printers"),
            text::body(fl!("no-printers-description")),
        ));
    } else {
        for printer in &page.printers {
            let item = crate::widget::go_next_with_item(
                &printer.name,
                text::body(printer_status_label(&printer.status)),
                Message::SelectPrinter(printer.clone()),
            );

            printers = printers.add(item);
        }
    }

    Element::from(
        column::with_capacity(3)
            .spacing(spacing.space_m)
            .push(header)
            .push(default_section)
            .push(printers),
    )
    .map(crate::pages::Message::Printers)
}
