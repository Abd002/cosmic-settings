use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::{
    Alignment, Subscription,
    futures::{SinkExt, StreamExt, channel::mpsc::Sender, future},
    stream,
};
use cosmic::widget::{
    button, column, dropdown, row, settings, space::horizontal as horizontal_space, text,
};
use cosmic_settings_page as page;
use cosmic_settings_printers_client::{self as printers_client, CosmicPrintersProxy};
pub use cosmic_settings_printers_core::{
    PrinterApplication, PrinterEntry, PrinterStatus, PrintersEvent, PrintersEventKind, SupplyLevel,
};
use slotmap::SlotMap;

pub mod add_printer;
#[allow(dead_code)]
mod backend;
pub mod details;
pub mod queue;
#[allow(dead_code)]
mod style;
#[allow(dead_code)]
mod widgets;

pub struct Page {
    entity: page::Entity,
    pub(crate) printers: Vec<PrinterEntry>,
    printer_applications: Vec<PrinterApplication>,
    pub(crate) default_printer_id: Option<String>,
    pub(crate) add_printer_dialog: Option<add_printer::Page>,
    details_page: page::Entity,
    default_printer_labels: Vec<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            printers: Vec::new(),
            printer_applications: Vec::new(),
            default_printer_id: None,
            add_printer_dialog: None,
            details_page: page::Entity::default(),
            default_printer_labels: default_printer_labels(&[]),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    OpenAddPrinterDialog,
    AddPrinter(add_printer::Message),
    DefaultPrinterDropdown(usize),
    Refresh,
    PrintersLoaded(Result<PrintersLoad, String>),
    PrintersEvent(PrintersEvent),
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
        self.add_printer_dialog.as_ref().map(add_printer::dialog)
    }

    fn on_enter(&mut self) -> cosmic::Task<crate::pages::Message> {
        cosmic::task::future(async {
            crate::pages::Message::Printers(Message::PrintersLoaded(load_printers().await))
        })
    }

    fn subscription(&self, _core: &cosmic::Core) -> Subscription<crate::pages::Message> {
        Subscription::run(printer_events_subscription).map(crate::pages::Message::Printers)
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
                self.add_printer_dialog = Some(add_printer::Page::new(
                    self.printers.clone(),
                    self.printer_applications.clone(),
                ));
                return add_printer::Page::load_task();
            }
            Message::AddPrinter(message) => {
                return self.update_add_printer(message);
            }
            Message::DefaultPrinterDropdown(idx) => {
                self.default_printer_id = idx
                    .checked_sub(1)
                    .and_then(|printer_idx| self.printers.get(printer_idx))
                    .map(|printer| printer.id.clone());
            }
            Message::Refresh => {
                return self.load_printers_task();
            }
            Message::PrintersLoaded(Ok(load)) => {
                self.printers = load.printers;
                self.printer_applications = load.printer_applications;
                self.default_printer_labels = default_printer_labels(&self.printers);
                if let Some(dialog) = &mut self.add_printer_dialog {
                    dialog.configured_printers = self.printers.clone();
                    dialog.printer_applications = self.printer_applications.clone();
                }
            }
            Message::PrintersLoaded(Err(why)) => {
                tracing::error!(why, "failed to load printers");
                self.printers.clear();
                self.printer_applications.clear();
                self.default_printer_id = None;
                self.default_printer_labels = default_printer_labels(&self.printers);
            }
            Message::PrintersEvent(event) => match event.kind {
                PrintersEventKind::DiscoveredPrintersChanged => {
                    let mut tasks = vec![self.load_printers_task()];
                    if self.add_printer_dialog.is_some() {
                        tasks.push(add_printer::Page::load_task());
                    }
                    return Task::batch(tasks);
                }
                PrintersEventKind::PrinterApplicationsChanged => {
                    return self.load_printers_task();
                }
            },
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

    fn update_add_printer(&mut self, message: add_printer::Message) -> Task<crate::Message> {
        let Some(dialog) = &mut self.add_printer_dialog else {
            return Task::none();
        };

        match dialog.update(message) {
            add_printer::Action::None => {}
            add_printer::Action::Close => {
                self.add_printer_dialog = None;
            }
            add_printer::Action::RefreshPrinters => {
                return self.load_printers_task();
            }
            add_printer::Action::Task(task) => {
                return task;
            }
        }

        Task::none()
    }

    fn load_printers_task(&self) -> Task<crate::Message> {
        cosmic::task::future(async {
            crate::Message::PageMessage(crate::pages::Message::Printers(Message::PrintersLoaded(
                load_printers().await,
            )))
        })
    }
}

#[derive(Clone, Debug)]
pub struct PrintersLoad {
    printers: Vec<PrinterEntry>,
    printer_applications: Vec<PrinterApplication>,
}

async fn load_printers() -> Result<PrintersLoad, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    let printers = client
        .conn
        .list_printers()
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))?;
    let printer_applications = client
        .conn
        .list_printer_applications()
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))?;

    Ok(PrintersLoad {
        printers: printers.printers,
        printer_applications: printer_applications.printer_applications,
    })
}

fn printer_events_subscription() -> impl futures::Stream<Item = Message> {
    stream::channel(8, |tx: Sender<Message>| async move {
        std::thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };

            runtime.block_on(watch_printer_events(tx));
        });

        future::pending::<()>().await;
    })
}

async fn watch_printer_events(mut tx: Sender<Message>) {
    let Ok(mut client) = printers_client::connect().await else {
        return;
    };

    let Ok(mut events) = client.conn.watch_printers().await else {
        return;
    };

    while let Some(event) = events.next().await {
        match event {
            Ok(Ok(event)) => {
                let _ = tx.send(Message::PrintersEvent(event)).await;
            }
            Ok(Err(why)) => {
                tracing::warn!(?why, "printer event stream returned an error");
                break;
            }
            Err(why) => {
                tracing::warn!(?why, "printer event stream failed");
                break;
            }
        }
    }
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

fn printer_application_web_page(application: &PrinterApplication) -> Option<String> {
    application
        .txt
        .get("adminurl")
        .filter(|url| !url.trim().is_empty())
        .cloned()
        .or_else(|| {
            let (scheme, rest) = application.system_uri.split_once("://")?;
            let authority = rest.split('/').next()?;
            let web_scheme = match scheme {
                "ipp" => "http",
                "ipps" => "https",
                _ => return None,
            };
            Some(format!("{web_scheme}://{authority}/"))
        })
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
