use cosmic::Element;
use cosmic::app::Task;
use cosmic::iced::{
    Alignment, Color, Length, Subscription, event,
    futures::{SinkExt, StreamExt, channel::mpsc::Sender, future},
    keyboard, stream,
};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{self, button, column, container, row, text};
use cosmic_settings_page as page;
use cosmic_settings_printers_client::{self as printers_client};
use cosmic_settings_printers_core::{GroupedDevice, group_printers};
pub use cosmic_settings_printers_core::{
    JobFilter, PrinterApplication, PrinterEntry, PrinterStatus, PrintersEvent, PrintersEventKind,
    SupplyLevel,
};
use slotmap::SlotMap;
use std::collections::HashMap;

pub mod add_printer;
mod backend;
pub mod details;
pub mod queue;
mod style;
mod widgets;

use style::{
    ACCENT, BODY_TEXT, FONT_BOLD, FONT_SEMIBOLD, NEUTRAL_WIDGET_BG, STATUS_PRINTING, STATUS_READY,
    STATUS_STOPPED, TITLE_TEXT,
};

const ADD_BUTTON_WIDTH: f32 = 107.0;
pub struct Page {
    entity: page::Entity,
    pub(crate) printers: Vec<PrinterEntry>,
    printer_applications: Vec<PrinterApplication>,
    pub(crate) default_printer_id: Option<String>,
    active_job_counts: HashMap<String, usize>,
    pub(crate) add_printer_dialog: Option<add_printer::Page>,
    details_page: page::Entity,
    queue_page: page::Entity,
    default_dropdown_open: bool,
    default_printer_labels: Vec<String>,
    printer_context: Option<String>,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            printers: Vec::new(),
            printer_applications: Vec::new(),
            default_printer_id: None,
            active_job_counts: HashMap::new(),
            add_printer_dialog: None,
            details_page: page::Entity::default(),
            queue_page: page::Entity::default(),
            default_dropdown_open: false,
            default_printer_labels: default_printer_labels(&[]),
            printer_context: None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    OpenAddPrinterDialog,
    AddPrinter(add_printer::Message),
    ToggleDefaultDropdown(bool),
    DefaultPrinterDropdown(usize),
    DefaultPrinterSet(Result<(), String>),
    Refresh,
    PrintersLoaded(Result<PrintersLoad, String>),
    JobsLoaded {
        printer_id: String,
        result: Result<usize, String>,
    },
    PrintersEvent(PrintersEvent),
    OpenPrinterSettings(PrinterEntry),
    OpenPrinterQueue(PrinterEntry),
    TogglePrinterContext(Option<String>),
    SetDefaultPrinter(String),
    OpenPrinterWebPage(String),
    PrinterWebPageOpened(Result<(), String>),
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
        page::Info::new("printers", "printer-symbolic").title(fl!("printers"))
    }

    fn header(&self) -> Option<Element<'_, crate::pages::Message>> {
        Some(page_header().map(crate::pages::Message::Printers))
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
        Subscription::batch([
            Subscription::run(printer_events_subscription).map(crate::pages::Message::Printers),
            event::listen_with(|event, _, _| match event {
                cosmic::iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                    Some(crate::pages::Message::PrinterQueue(
                        queue::Message::ModifiersChanged(modifiers),
                    ))
                }
                _ => None,
            }),
        ])
    }

    fn content(
        &self,
        sections: &mut SlotMap<page::section::Entity, page::Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(
            page::Section::default().view::<Page>(|_binder, page, _| view(page)),
        )])
    }
}

impl Page {
    fn default_printer_selection(&self) -> Option<usize> {
        let selected = self
            .default_printer_id
            .as_deref()
            .and_then(|default_id| {
                self.printers
                    .iter()
                    .position(|printer| printer.id() == default_id)
            })
            .map_or(0, |index| index + 1);

        Some(selected)
    }
}

impl page::AutoBind<crate::pages::Message> for Page {
    fn sub_pages(
        mut page: page::Insert<crate::pages::Message>,
    ) -> page::Insert<crate::pages::Message> {
        let details_id = page.sub_page_with_id::<details::Page>();
        let queue_id = page.sub_page_with_id::<queue::Page>();

        if let Some(model) = page.model.page_mut::<Page>() {
            model.details_page = details_id;
            model.queue_page = queue_id;
        }

        page
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::OpenAddPrinterDialog => self.open_add_printer_dialog(),
            Message::AddPrinter(message) => self.update_add_printer(message),
            Message::DefaultPrinterDropdown(index) => self.select_default_printer(index),
            Message::SetDefaultPrinter(printer_id) => {
                self.printer_context = None;
                self.default_printer_id = Some(printer_id.clone());
                set_default_printer_task(printer_id)
            }
            Message::DefaultPrinterSet(Ok(())) => Self::load_printers_task(),
            Message::DefaultPrinterSet(Err(why)) => {
                tracing::warn!(why, "failed to set default printer");
                Self::load_printers_task()
            }
            Message::ToggleDefaultDropdown(open) => {
                self.default_dropdown_open = open;
                Task::none()
            }
            Message::Refresh => Self::load_printers_task(),
            Message::PrintersLoaded(Ok(load)) => self.apply_printers_load(load),
            Message::PrintersLoaded(Err(why)) => {
                self.clear_printers_after_load_error(why);
                Task::none()
            }
            Message::JobsLoaded { printer_id, result } => {
                self.apply_active_job_count(printer_id, result);
                Task::none()
            }
            Message::PrintersEvent(event) => self.handle_printers_event(event),
            Message::OpenPrinterSettings(printer) => self.open_printer_settings(printer),
            Message::OpenPrinterQueue(printer) => self.open_printer_queue(printer),
            Message::TogglePrinterContext(printer_id) => {
                self.printer_context = printer_id;
                Task::none()
            }
            Message::OpenPrinterWebPage(web_page) => Self::open_printer_web_page(web_page),
            Message::PrinterWebPageOpened(result) => {
                if let Err(why) = result {
                    tracing::warn!(why, "failed to open printer web page");
                }
                Task::none()
            }
        }
    }

    fn open_add_printer_dialog(&mut self) -> Task<crate::Message> {
        self.add_printer_dialog = Some(add_printer::Page::new(self.printers.clone()));
        add_printer::Page::load_task()
    }

    fn select_default_printer(&mut self, index: usize) -> Task<crate::Message> {
        self.default_dropdown_open = false;
        let printer_id = index
            .checked_sub(1)
            .and_then(|printer_index| self.printers.get(printer_index))
            .map(|printer| printer.id().to_string());
        self.default_printer_id = printer_id.clone();

        if let Some(printer_id) = printer_id {
            return set_default_printer_task(printer_id);
        }

        // TODO: Add an unset-default operation to cosmic-printers.
        // Reload instead of leaving an unsupported optimistic state.
        Self::load_printers_task()
    }

    fn apply_printers_load(&mut self, load: PrintersLoad) -> Task<crate::Message> {
        self.default_printer_id = load
            .printers
            .iter()
            .find(|printer| printer.is_default())
            .map(|printer| printer.id().to_string());
        self.active_job_counts.retain(|printer_id, _| {
            load.printers
                .iter()
                .any(|printer| printer.id() == printer_id)
        });
        self.printers = load.printers;
        self.printer_applications = load.printer_applications;
        self.default_printer_labels = default_printer_labels(&self.printers);

        if let Some(dialog) = &mut self.add_printer_dialog {
            dialog.configured_printers = self.printers.clone();
        }

        self.load_active_jobs_task()
    }

    fn clear_printers_after_load_error(&mut self, why: String) {
        tracing::error!(why, "failed to load printers");
        self.printers.clear();
        self.printer_applications.clear();
        self.default_printer_id = None;
        self.active_job_counts.clear();
        self.default_printer_labels = default_printer_labels(&self.printers);
    }

    fn apply_active_job_count(&mut self, printer_id: String, result: Result<usize, String>) {
        match result {
            Ok(count) => {
                self.active_job_counts.insert(printer_id, count);
            }
            Err(why) => {
                tracing::warn!(printer_id, why, "failed to load active printer jobs");
            }
        }
    }

    fn handle_printers_event(&self, event: PrintersEvent) -> Task<crate::Message> {
        match event.kind {
            // The dialog shows configured printers too, and a destination appearing
            // or going away changes what it should say.
            PrintersEventKind::AvailableDestinationsChanged
            | PrintersEventKind::PrinterApplicationsChanged => Task::batch([
                Self::load_printers_task(),
                self.add_printer_task(add_printer::Page::refresh_task),
            ]),
            PrintersEventKind::AddPrinterDiscoveryChanged => {
                self.add_printer_task(add_printer::Page::refresh_task)
            }
            PrintersEventKind::PrinterConfigurationChanged => self.add_printer_task(|| {
                cosmic::task::message(crate::Message::PageMessage(
                    add_printer::Message::ConfigurationChanged.into(),
                ))
            }),
        }
    }

    fn add_printer_task(
        &self,
        task: impl FnOnce() -> Task<crate::Message>,
    ) -> Task<crate::Message> {
        if self.add_printer_dialog.is_some() {
            task()
        } else {
            Task::none()
        }
    }

    fn open_printer_settings(&mut self, printer: PrinterEntry) -> Task<crate::Message> {
        self.printer_context = None;
        let is_default = self.default_printer_id.as_deref() == Some(printer.id());

        Task::batch([
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::PrinterDetails(details::Message::LoadPrinter {
                    printer,
                    is_default,
                    parent_page: self.entity,
                    queue_page: self.queue_page,
                    available_printers: self.printers.clone(),
                }),
            )),
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::Page(self.details_page),
            )),
        ])
    }

    fn open_printer_queue(&mut self, printer: PrinterEntry) -> Task<crate::Message> {
        self.printer_context = None;

        Task::batch([
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::PrinterQueue(queue::Message::LoadPrinter {
                    printer: Box::new(printer),
                    available_printers: self.printers.clone(),
                }),
            )),
            cosmic::task::message(crate::app::Message::OpenContextDrawer(self.queue_page)),
        ])
    }

    fn open_printer_web_page(web_page: String) -> Task<crate::Message> {
        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::PrinterWebPageOpened(backend::open_printer_web_page(web_page).await),
            ))
        })
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
                return Self::load_printers_task();
            }
            // A printer that has just been set up is no longer something to add, and
            // whether it is set up is something only a fresh round establishes.
            add_printer::Action::RediscoverPrinters => {
                return Task::batch([Self::load_printers_task(), add_printer::Page::load_task()]);
            }
            add_printer::Action::Task(task) => {
                return task;
            }
        }

        Task::none()
    }

    fn load_printers_task() -> Task<crate::Message> {
        cosmic::task::future(async {
            crate::Message::PageMessage(crate::pages::Message::Printers(Message::PrintersLoaded(
                load_printers().await,
            )))
        })
    }

    fn load_active_jobs_task(&self) -> Task<crate::Message> {
        Task::batch(self.printers.iter().map(|printer| {
            let printer_id = printer.id().to_string();

            cosmic::task::future(async move {
                let result = load_active_job_count(printer_id.clone()).await;
                crate::Message::PageMessage(crate::pages::Message::Printers(Message::JobsLoaded {
                    printer_id,
                    result,
                }))
            })
        }))
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
    client
        .refresh_available_destinations()
        .await
        .map_err(|why| why.to_string())?;
    client
        .start_printer_application_discovery()
        .await
        .map_err(|why| why.to_string())?;
    let printers = client.printers().await.map_err(|why| why.to_string())?;
    let printer_applications = client
        .printer_applications()
        .await
        .map_err(|why| why.to_string())?;

    Ok(PrintersLoad {
        printers,
        printer_applications,
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

    let Ok(mut events) = client.printer_events().await else {
        return;
    };

    while let Some(event) = events.next().await {
        match event {
            Ok(event) => {
                let _ = tx.send(Message::PrintersEvent(event)).await;
            }
            Err(why) => {
                tracing::warn!(?why, "printer event stream failed");
                break;
            }
        }
    }
}

async fn load_active_job_count(printer_id: String) -> Result<usize, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    let jobs = client
        .jobs(&printer_id, JobFilter::Active)
        .await
        .map_err(|why| why.to_string())?;

    Ok(jobs.len())
}

fn default_printer_labels(printers: &[PrinterEntry]) -> Vec<String> {
    std::iter::once(fl!("default-printer-not-set"))
        .chain(printers.iter().map(|printer| printer.name().to_string()))
        .collect()
}

fn view(page: &Page) -> Element<'_, crate::pages::Message> {
    Element::from(
        widget::responsive(move |size| {
            let padding = adaptive_inner_padding(size.width);
            container(page_content(page))
                .padding([0, padding, 32, padding])
                .width(Length::Fill)
                .align_y(Alignment::Start)
                .into()
        })
        .width(Length::Fill)
        .height(Length::Shrink),
    )
    .map(crate::pages::Message::Printers)
}

fn page_content(page: &Page) -> Element<'_, Message> {
    let groups = group_printers(page.printers.clone(), page.printer_applications.clone());
    let mut cards = column::with_capacity(groups.len().max(1))
        .spacing(12)
        .width(Length::Fill);

    if groups.is_empty() {
        cards = cards.push(empty_printers_card());
    } else {
        for group in groups {
            cards = cards.push(printer_card(page, group));
        }
    }

    column::with_capacity(2)
        .push(default_printer_row(page))
        .push(cards)
        .spacing(24)
        .width(Length::Fill)
        .into()
}

fn page_header<'a>() -> Element<'a, Message> {
    widget::responsive(|size| {
        let title = text::body(fl!("printers"))
            .size(29)
            .font(FONT_SEMIBOLD)
            .class(TITLE_TEXT)
            .width(Length::Fill)
            .height(Length::Fixed(43.0))
            .align_y(Alignment::Center);
        let add_button = button::custom(
            container(text::body(fl!("add-printer")).size(14).class(TITLE_TEXT))
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill),
        )
        .padding([0, 16])
        .width(Length::Fixed(ADD_BUTTON_WIDTH))
        .height(Length::Fixed(32.0))
        .class(widgets::pill_button_style(NEUTRAL_WIDGET_BG, TITLE_TEXT))
        .on_press(Message::OpenAddPrinterDialog);

        container(
            row::with_capacity(2)
                .push(title)
                .push(add_button)
                .align_y(Alignment::Center)
                .spacing(16)
                .width(Length::Fill)
                .height(Length::Fixed(43.0)),
        )
        .padding([0, adaptive_inner_padding(size.width)])
        .width(Length::Fill)
        .height(Length::Fixed(43.0))
        .align_y(Alignment::Start)
        .into()
    })
    .width(Length::Fill)
    .height(Length::Fixed(43.0))
    .into()
}

pub(super) fn adaptive_inner_padding(available_width: f32) -> u16 {
    if available_width > 608.0 { 32 } else { 0 }
}

fn default_printer_row(page: &Page) -> Element<'_, Message> {
    let label = text::body(fl!("default-printer"))
        .size(14)
        .class(BODY_TEXT)
        .width(Length::Fill);

    let dropdown = widgets::dropdown_action(
        selected_default_printer_label(page),
        page.default_printer_labels.clone(),
        page.default_printer_selection(),
        page.default_dropdown_open,
        Message::ToggleDefaultDropdown,
        Message::DefaultPrinterDropdown,
        widgets::DropdownWidths {
            trigger: 147.0,
            popup: 260.0,
        },
    );

    widgets::card_container(
        row::with_capacity(2)
            .push(label)
            .push(dropdown)
            .align_y(Alignment::Center)
            .padding([0, 24])
            .height(Length::Fixed(48.0))
            .width(Length::Fill),
    )
    .into()
}

fn empty_printers_card<'a>() -> Element<'a, Message> {
    container(
        text::body(fl!("no-printers-found"))
            .size(14)
            .class(BODY_TEXT),
    )
    .padding([16, 24])
    .center_x(Length::Fill)
    .center_y(Length::Fixed(53.0))
    .width(Length::Fill)
    .height(Length::Fixed(53.0))
    .class(widgets::fill_container(style::CARD_BG, style::RADIUS_CARD))
    .into()
}

fn printer_card(page: &Page, group: GroupedDevice) -> Element<'static, Message> {
    let application = group.printer_application().cloned();
    let printers = group.queues().to_vec();
    let mut card = column::with_capacity(printers.len().saturating_mul(2) + 2).width(Length::Fill);

    if let Some(application) = application.as_ref() {
        card = card.push(printer_application_header(application));
        if !printers.is_empty() {
            card = card.push(widgets::divider());
        }
    }

    for (index, printer) in printers.iter().enumerate() {
        card = card.push(printer_destination(page, printer));

        if index + 1 < printers.len() {
            card = card.push(widgets::divider());
        }
    }

    widgets::card_container(card).into()
}

fn printer_application_header(application: &PrinterApplication) -> Element<'static, Message> {
    let title = single_line(
        non_empty(&application.service_name)
            .map(str::to_owned)
            .unwrap_or_else(|| fl!("generic-printer-application")),
        14,
        BODY_TEXT,
    )
    .font(FONT_BOLD);
    let mut header = row::with_capacity(2)
        .push(title)
        .align_y(Alignment::Center)
        .spacing(4);

    if let Some(web_page) = printer_application_web_page(application) {
        header = header.push(icon_button(
            "view-web-browser-symbolic",
            Message::OpenPrinterWebPage(web_page),
        ));
    }

    container(header)
        .padding([8, 24])
        .height(Length::Fixed(48.0))
        .width(Length::Fill)
        .into()
}

fn printer_destination(page: &Page, printer: &PrinterEntry) -> Element<'static, Message> {
    let mut name_col = column::with_capacity(2).push(
        single_line(printer.name().to_string(), 20, BODY_TEXT)
            .font(FONT_BOLD)
            .height(Length::Fixed(30.0)),
    );

    if let Some(subtitle) = printer_subtitle(printer) {
        name_col = name_col.push(single_line(subtitle, 14, BODY_TEXT).height(Length::Fixed(21.0)));
    }

    let (status_label, status_color) = visual_status(page, printer);
    let status_row = row::with_capacity(2)
        .push(widgets::dot(status_color, 8.0))
        .push(text::body(status_label).size(14).class(BODY_TEXT))
        .spacing(4)
        .align_y(Alignment::Center);

    let information = column::with_capacity(2)
        .push(name_col)
        .push(
            column::with_capacity(2).push(status_row).push(
                text::body(job_detail(page, printer))
                    .size(14)
                    .class(BODY_TEXT),
            ),
        )
        .spacing(8)
        .width(Length::Fill);
    let destination = column::with_capacity(2)
        .push(information)
        .push(printer_destination_actions(printer))
        .spacing(16)
        .padding([16, 24])
        .width(Length::Fill);
    let trigger = widget::mouse_area(destination).on_right_press(Message::TogglePrinterContext(
        Some(printer.id().to_string()),
    ));

    if page.printer_context.as_deref() == Some(printer.id()) {
        widget::popover(trigger)
            .position(widget::popover::Position::Bottom)
            .popup(printer_context_menu(page, printer))
            .on_close(Message::TogglePrinterContext(None))
            .into()
    } else {
        trigger.into()
    }
}

fn printer_destination_actions(printer: &PrinterEntry) -> Element<'static, Message> {
    let mut left = row::with_capacity(2).spacing(4).align_y(Alignment::Center);
    if let Some(web_page) = printer.web_page() {
        left = left.push(icon_button(
            "view-web-browser-symbolic",
            Message::OpenPrinterWebPage(web_page.to_string()),
        ));
    }
    left = left.push(icon_button(
        "view-printer-queue-symbolic",
        Message::OpenPrinterQueue(printer.clone()),
    ));

    let settings = button::custom(
        row::with_capacity(2)
            .push(text::body(fl!("settings")).size(14).class(ACCENT))
            .push(widgets::symbolic_icon("go-next-symbolic", 16, ACCENT))
            .spacing(4)
            .align_y(Alignment::Center),
    )
    .padding([0, 16])
    .width(Length::Fixed(104.0))
    .height(Length::Fixed(32.0))
    .class(cosmic::theme::Button::Link)
    .on_press(Message::OpenPrinterSettings(printer.clone()));

    row::with_capacity(2)
        .push(left.width(Length::Fill))
        .push(settings)
        .align_y(Alignment::Center)
        .spacing(8)
        .height(Length::Fixed(32.0))
        .width(Length::Fill)
        .into()
}

fn icon_button(name: &'static str, message: Message) -> Element<'static, Message> {
    button::custom(widgets::symbolic_icon(name, 16, BODY_TEXT))
        .padding(8)
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .class(cosmic::theme::Button::Transparent)
        .on_press(message)
        .into()
}

fn printer_context_menu(page: &Page, printer: &PrinterEntry) -> Element<'static, Message> {
    let is_default = page.default_printer_id.as_deref() == Some(printer.id());
    let rows = column::with_capacity(7)
        .push(context_menu_row(
            fl!("set-as-default-printer"),
            (!is_default).then(|| Message::SetDefaultPrinter(printer.id().to_string())),
        ))
        .push(widgets::inset_divider(8))
        .push(context_menu_row(
            fl!("printer-queue"),
            Some(Message::OpenPrinterQueue(printer.clone())),
        ))
        .push(widgets::inset_divider(8))
        .push(context_menu_row(
            fl!("printer-settings"),
            Some(Message::OpenPrinterSettings(printer.clone())),
        ))
        .push(widgets::inset_divider(8))
        .push(context_menu_row(
            fl!("printer-web-interface"),
            printer
                .web_page()
                .map(|web_page| Message::OpenPrinterWebPage(web_page.to_string())),
        ));

    container(rows)
        .padding([8, 0])
        .width(Length::Fixed(360.0))
        .class(widgets::context_menu_container())
        .into()
}

fn context_menu_row(label: String, message: Option<Message>) -> Element<'static, Message> {
    widgets::context_menu_row(single_line(label, 14, BODY_TEXT), message, 40.0)
}

fn selected_default_printer_label(page: &Page) -> String {
    page.default_printer_selection()
        .and_then(|idx| page.default_printer_labels.get(idx))
        .cloned()
        .unwrap_or_else(|| fl!("default-printer-not-set"))
}

fn printer_subtitle(printer: &PrinterEntry) -> Option<String> {
    printer
        .model()
        .and_then(non_empty)
        .filter(|model| *model != printer.name())
        .map(str::to_owned)
}

fn active_job_count(page: &Page, printer: &PrinterEntry) -> usize {
    page.active_job_counts
        .get(printer.id())
        .copied()
        .unwrap_or_default()
}

fn visual_status(page: &Page, printer: &PrinterEntry) -> (String, Color) {
    if active_job_count(page, printer) > 0 {
        return (fl!("printer-printing"), STATUS_PRINTING);
    }

    match printer.status() {
        PrinterStatus::Ready => (fl!("printer-ready"), STATUS_READY),
        PrinterStatus::Offline => (fl!("printer-stopped"), STATUS_STOPPED),
        PrinterStatus::LowToner => (fl!("printer-low-toner"), STATUS_STOPPED),
    }
}

fn job_detail(page: &Page, printer: &PrinterEntry) -> String {
    if !matches!(printer.status(), PrinterStatus::Ready)
        && let Some(reason) = printer.queue_status().and_then(non_empty)
    {
        return reason.to_owned();
    }

    match active_job_count(page, printer) {
        0 => fl!("no-jobs-waiting"),
        1 => fl!("one-document"),
        count => fl!("documents-count", count = count),
    }
}

fn printer_application_web_page(application: &PrinterApplication) -> Option<String> {
    application
        .txt
        .get("adminurl")
        .and_then(|url| non_empty(url))
        .map(str::to_owned)
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

fn single_line(
    label: String,
    size: u16,
    color: Color,
) -> cosmic::widget::text::Text<'static, cosmic::Theme> {
    text::body(label)
        .size(size)
        .class(cosmic::theme::Text::Color(color))
        .width(Length::Fill)
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
}

fn set_default_printer_task(printer_id: String) -> Task<crate::Message> {
    cosmic::task::future(async move {
        crate::Message::PageMessage(crate::pages::Message::Printers(Message::DefaultPrinterSet(
            backend::set_printer_default(printer_id).await,
        )))
    })
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}
