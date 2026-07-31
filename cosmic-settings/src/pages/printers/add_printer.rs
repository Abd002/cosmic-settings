use std::rc::Rc;

use cosmic::app::Task;
use cosmic::iced::widget::scrollable::{Direction, Scrollbar};
use cosmic::iced::{Alignment, Background, Color, Length, Shadow, Vector};
use cosmic::iced_core::Border;
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{
    self, button, column, container, icon, row, scrollable, space::horizontal as horizontal_space,
    text,
};
use cosmic::{Apply, Element};
use cosmic_settings_printers_client::{self as printers_client};
use cosmic_settings_printers_core::{PrinterApplication, PrinterEntry};

use super::backend;
use super::style::{
    ACCENT, BODY_TEXT, BORDER_SUBTLE, BUTTON_CANCEL, DARK_DIALOG, DARK_FOOTER, DARK_LIST,
    RADIUS_CARD, TEXT_MUTED,
};
use super::widgets;

const DIALOG_WIDTH: f32 = 680.0;
const DIALOG_HEIGHT: f32 = 570.0;
const CONTENT_HEIGHT: f32 = 506.0;
const CONTENT_WIDTH: f32 = 552.0;
const PRINTER_ROW_HEIGHT: f32 = 54.0;
const APPLICATION_ROW_HEIGHT: f32 = 48.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogView {
    Discovery,
    SelectApplication {
        printer_id: String,
        application_ids: Vec<String>,
    },
    ManualSetup,
    Adding {
        printer_id: String,
        application_id: Option<String>,
    },
    Added,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub search: String,
    pub loading: bool,
    pub error: Option<String>,
    pub configured_printers: Vec<PrinterEntry>,
    pub discovered_printers: Vec<PrinterEntry>,
    pub printer_applications: Vec<PrinterApplication>,
    pub view: DialogView,
    pub added_printers: Vec<PrinterEntry>,
}

impl Page {
    pub fn new(
        configured_printers: Vec<PrinterEntry>,
        printer_applications: Vec<PrinterApplication>,
    ) -> Self {
        Self {
            search: String::new(),
            loading: true,
            error: None,
            configured_printers,
            discovered_printers: Vec::new(),
            printer_applications,
            view: DialogView::Discovery,
            added_printers: Vec::new(),
        }
    }

    pub fn visible_printers(&self) -> impl Iterator<Item = &PrinterEntry> {
        let search = self.search.trim().to_lowercase();

        self.discovered_printers.iter().filter(move |printer| {
            !self.printer_is_configured(printer) && printer_matches_search(printer, &search)
        })
    }

    pub fn load_task() -> Task<crate::Message> {
        cosmic::task::future(async {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::DiscoveredPrintersLoaded(load_discovered_printers().await).into(),
            ))
        })
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Close => Action::Close,
            Message::Search(search) => {
                self.search = search;
                Action::None
            }
            Message::DiscoveredPrintersLoaded(result) => {
                self.apply_discovered_printers(result);
                Action::None
            }
            Message::OpenManualSetup => {
                self.open_manual_setup();
                Action::None
            }
            Message::SelectDiscoveredPrinter(printer_id) => {
                self.select_discovered_printer(printer_id)
            }
            Message::SelectPrinterApplication(application_id) => {
                self.select_printer_application(application_id)
            }
            Message::OpenPrinterApplication(application_id) => {
                self.open_printer_application(application_id)
            }
            Message::OpenPrinterWebPage(web_page) => Self::open_web_page(web_page),
            Message::PrinterSetupFinished(result) => self.finish_printer_setup(result),
            Message::WebPageOpened(result) => self.finish_web_page_open(result),
        }
    }

    fn apply_discovered_printers(&mut self, result: Result<Vec<PrinterEntry>, String>) {
        match result {
            Ok(printers) => {
                self.loading = false;
                self.error = None;
                self.discovered_printers = printers;
                if self.view == DialogView::Discovery && self.visible_printers().next().is_none() {
                    self.view = DialogView::ManualSetup;
                }
            }
            Err(why) => {
                tracing::error!(why, "failed to discover printers");
                self.loading = false;
                self.error = Some(fl!("failed-to-load-printers"));
                self.discovered_printers.clear();
            }
        }
    }

    fn open_manual_setup(&mut self) {
        if self.is_adding() {
            return;
        }

        self.error = None;
        self.view = DialogView::ManualSetup;
    }

    fn select_discovered_printer(&mut self, printer_id: String) -> Action {
        if self.loading || self.is_adding() {
            return Action::None;
        }

        let Some(printer) = self.discovered_printer(&printer_id) else {
            self.error = Some(fl!("no-compatible-printer-applications"));
            return Action::None;
        };
        let application_ids = self.application_ids_for_printer(printer);

        match application_ids.as_slice() {
            [] => {
                self.error = Some(fl!("no-compatible-printer-applications"));
                Action::None
            }
            [application_id] => self.start_setup(printer_id, Some(application_id.clone())),
            _ => {
                self.error = None;
                self.view = DialogView::SelectApplication {
                    printer_id,
                    application_ids,
                };
                Action::None
            }
        }
    }

    fn select_printer_application(&mut self, application_id: String) -> Action {
        let printer_id = match &self.view {
            DialogView::SelectApplication {
                printer_id,
                application_ids,
            } if application_ids.contains(&application_id) => printer_id.clone(),
            _ => return Action::None,
        };

        self.start_setup(printer_id, Some(application_id))
    }

    fn open_printer_application(&mut self, application_id: String) -> Action {
        let Some(web_page) = self
            .printer_application(&application_id)
            .and_then(super::printer_application_web_page)
        else {
            self.error = Some(fl!("printer-application-web-interface-unavailable"));
            return Action::None;
        };

        Self::open_web_page(web_page)
    }

    fn finish_printer_setup(&mut self, result: Result<PrinterEntry, String>) -> Action {
        match result {
            Ok(printer) => {
                self.error = None;
                self.added_printers.push(printer);
                self.view = DialogView::Added;
                Action::RefreshPrinters
            }
            Err(why) => {
                tracing::error!(why, "failed to add discovered printer");
                self.error = Some(why);
                self.view = DialogView::Discovery;
                Action::None
            }
        }
    }

    fn finish_web_page_open(&mut self, result: Result<(), String>) -> Action {
        match result {
            Ok(()) => Action::None,
            Err(why) => {
                tracing::error!(why, "failed to open printer web page");
                self.error = Some(why);
                Action::None
            }
        }
    }

    fn discovered_printer(&self, printer_id: &str) -> Option<&PrinterEntry> {
        self.discovered_printers
            .iter()
            .find(|printer| discovered_printer_id(printer) == printer_id)
    }

    fn printer_application(&self, application_id: &str) -> Option<&PrinterApplication> {
        self.printer_applications
            .iter()
            .find(|application| application.id == application_id)
    }

    fn application_ids_for_printer(&self, printer: &PrinterEntry) -> Vec<String> {
        // TODO: Replace endpoint association with backend-provided device/application
        // matches once PAPPL-Find-Devices observations are exposed by the daemon.
        self.printer_applications
            .iter()
            .filter(|application| application_matches_printer(application, printer))
            .map(|application| application.id.clone())
            .collect()
    }

    fn printer_is_configured(&self, printer: &PrinterEntry) -> bool {
        self.configured_printers
            .iter()
            .any(|configured| discovered_queue_matches(configured, printer))
    }

    fn is_adding(&self) -> bool {
        matches!(self.view, DialogView::Adding { .. })
    }

    fn start_setup(&mut self, printer_id: String, application_id: Option<String>) -> Action {
        if self.is_adding() {
            return Action::None;
        }

        let Some(printer) = self.discovered_printer(&printer_id).cloned() else {
            self.error = Some(fl!("no-printers-found"));
            return Action::None;
        };

        self.error = None;
        self.view = DialogView::Adding {
            printer_id: printer_id.clone(),
            application_id: application_id.clone(),
        };

        Action::Task(cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::PrinterSetupFinished(
                    setup_discovered_printer(printer_id, printer, application_id).await,
                )
                .into(),
            ))
        }))
    }

    fn open_web_page(web_page: String) -> Action {
        Action::Task(cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::WebPageOpened(backend::open_printer_web_page(web_page).await).into(),
            ))
        }))
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Close,
    Search(String),
    DiscoveredPrintersLoaded(Result<Vec<PrinterEntry>, String>),
    OpenManualSetup,
    SelectDiscoveredPrinter(String),
    SelectPrinterApplication(String),
    OpenPrinterApplication(String),
    OpenPrinterWebPage(String),
    PrinterSetupFinished(Result<PrinterEntry, String>),
    WebPageOpened(Result<(), String>),
}

impl From<Message> for super::Message {
    fn from(message: Message) -> Self {
        super::Message::AddPrinter(message)
    }
}

impl From<Message> for crate::pages::Message {
    fn from(message: Message) -> Self {
        super::Message::AddPrinter(message).into()
    }
}

pub enum Action {
    None,
    Close,
    RefreshPrinters,
    Task(Task<crate::Message>),
}

async fn load_discovered_printers() -> Result<Vec<PrinterEntry>, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    client
        .start_discovery()
        .await
        .map_err(|why| why.to_string())?;
    client
        .discovered_printers()
        .await
        .map_err(|why| why.to_string())
}

async fn setup_discovered_printer(
    printer_id: String,
    discovered: PrinterEntry,
    application_id: Option<String>,
) -> Result<PrinterEntry, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;

    // TODO: Pass application_id once the daemon add API supports selecting a
    // specific Printer Application. The state machine already preserves it.
    let _ = application_id;
    client
        .add_discovered_printer(&printer_id)
        .await
        .map_err(|why| why.to_string())?;

    let configured = client.printers().await.map_err(|why| why.to_string())?;

    configured
        .into_iter()
        .find(|printer| discovered_queue_matches(printer, &discovered))
        .ok_or_else(|| fl!("configured-printer-not-found"))
}

pub fn dialog(page: &Page) -> Element<'_, crate::pages::Message> {
    let body = match &page.view {
        DialogView::Discovery | DialogView::Adding { .. } => discovery_view(page),
        DialogView::ManualSetup => manual_setup_view(page),
        DialogView::SelectApplication {
            application_ids, ..
        } => select_application_view(page, application_ids),
        DialogView::Added => added_printers_view(page),
    };
    let footer_label = match page.view {
        DialogView::ManualSetup | DialogView::Added => fl!("close"),
        _ => fl!("cancel"),
    };

    column::with_capacity(2)
        .push(body)
        .push(dialog_footer(footer_label))
        .width(Length::Fixed(DIALOG_WIDTH))
        .height(Length::Fixed(DIALOG_HEIGHT))
        .apply(container)
        .width(Length::Fixed(DIALOG_WIDTH))
        .height(Length::Fixed(DIALOG_HEIGHT))
        .class(dialog_container())
        .apply(Element::from)
        .map(super::Message::from)
        .map(crate::pages::Message::Printers)
}

fn discovery_view(page: &Page) -> Element<'_, Message> {
    let search_input = widget::search_input(fl!("type-to-search"), &page.search)
        .on_input(Message::Search)
        .on_clear(Message::Search(String::new()))
        .width(Length::Fixed(314.0));
    let search = container(search_input)
        .width(Length::Fill)
        .height(Length::Fixed(32.0))
        .center_x(Length::Fill);

    let mut settings = column::with_capacity(3)
        .spacing(16)
        .push(search)
        .push(printers_section(page));
    if !page.loading {
        settings = settings.push(manual_setup_prompt());
    }

    padded_content(
        settings
            .width(Length::Fixed(CONTENT_WIDTH))
            .height(Length::Fixed(442.0)),
        [32, 64],
    )
}

fn manual_setup_view(page: &Page) -> Element<'_, Message> {
    let mut rows = Vec::with_capacity(page.printer_applications.len().max(1) + 1);
    if let Some(error) = &page.error {
        rows.push(plain_row(error.clone()));
    }
    rows.extend(page.printer_applications.iter().map(manual_application_row));
    if rows.is_empty() {
        rows.push(plain_row(fl!("no-printer-applications-found")));
    }
    let rows = with_dividers(rows);

    padded_content(
        column::with_capacity(2)
            .spacing(8)
            .push(regular_heading(fl!(
                "use-a-printer-application-to-manually-set-up-a-printer"
            )))
            .push(application_list(rows)),
        [64, 64],
    )
}

fn select_application_view<'a>(page: &'a Page, application_ids: &[String]) -> Element<'a, Message> {
    let rows = with_dividers(
        application_ids
            .iter()
            .filter_map(|id| page.printer_application(id))
            .map(select_application_row)
            .collect(),
    );

    padded_content(
        column::with_capacity(2)
            .spacing(8)
            .push(regular_heading(fl!(
                "choose-the-printer-application-to-set-up-your-printer"
            )))
            .push(application_list(if rows.is_empty() {
                vec![plain_row(fl!("no-printer-applications-found"))]
            } else {
                rows
            })),
        [64, 64],
    )
}

fn added_printers_view(page: &Page) -> Element<'_, Message> {
    let rows = with_dividers(page.added_printers.iter().map(added_printer_row).collect());
    let description = row::with_capacity(2)
        .spacing(4)
        .align_y(Alignment::End)
        .push(
            text::body(fl!("printer-web-interface-description"))
                .class(cosmic::theme::Text::Color(TEXT_MUTED))
                .width(Length::Fill),
        )
        .push(
            icon::from_name("view-web-browser-symbolic")
                .size(16)
                .icon()
                .class(cosmic::theme::Svg::Custom(primary_svg())),
        );

    padded_content(
        column::with_capacity(2)
            .spacing(8)
            .push(
                column::with_capacity(2)
                    .spacing(8)
                    .push(section_heading(fl!("added-printers")))
                    .push(list_view(rows)),
            )
            .push(description),
        [32, 64],
    )
}

fn padded_content<'a>(
    content: impl Into<Element<'a, Message>>,
    padding: [u16; 2],
) -> Element<'a, Message> {
    container(content)
        .padding(padding)
        .width(Length::Fill)
        .height(Length::Fixed(CONTENT_HEIGHT))
        .into()
}

fn printers_section(page: &Page) -> Element<'_, Message> {
    let (rows, printer_count, row_height) = if page.loading {
        (vec![plain_row(fl!("searching"))], 1, APPLICATION_ROW_HEIGHT)
    } else if let Some(error) = &page.error {
        (vec![plain_row(error.clone())], 1, APPLICATION_ROW_HEIGHT)
    } else {
        let printers = page.visible_printers().collect::<Vec<_>>();
        let rows = if printers.is_empty() {
            vec![plain_row(fl!("no-printers-found"))]
        } else {
            with_dividers(
                printers
                    .iter()
                    .map(|printer| discovered_printer_row(page, printer))
                    .collect(),
            )
        };
        (rows, printers.len().max(1), PRINTER_ROW_HEIGHT)
    };
    let list_height = list_height(printer_count, row_height, 4);

    column::with_capacity(2)
        .spacing(8)
        .push(section_heading(fl!("printers")))
        .push(
            scrollable(list_view(rows))
                .direction(Direction::Vertical(Scrollbar::hidden()))
                .height(Length::Fixed(list_height)),
        )
        .into()
}

fn manual_setup_prompt() -> Element<'static, Message> {
    column::with_capacity(2)
        .spacing(8)
        .push(regular_heading(fl!("your-printer-not-discovered")))
        .push(
            button::custom(centered_label(fl!("manual-setup"), TEXT_MUTED))
                .padding([0, 16])
                .width(Length::Fixed(122.0))
                .height(Length::Fixed(32.0))
                .class(widgets::pill_button_style(DARK_FOOTER, TEXT_MUTED))
                .on_press(Message::OpenManualSetup),
        )
        .into()
}

fn discovered_printer_row(page: &Page, printer: &PrinterEntry) -> Element<'static, Message> {
    let printer_id = discovered_printer_id(printer);
    let connecting = matches!(
        &page.view,
        DialogView::Adding {
            printer_id: adding_id,
            ..
        } if adding_id == &printer_id
    );
    let status = if connecting {
        fl!("connecting")
    } else {
        printer_location_label(printer)
    };
    let content = two_line_printer_content(printer, status, connecting, None);

    button::custom(content)
        .padding([8, 24])
        .width(Length::Fill)
        .height(Length::Fixed(PRINTER_ROW_HEIGHT))
        .class(cosmic::theme::Button::Transparent)
        .on_press_maybe((!page.is_adding()).then_some(Message::SelectDiscoveredPrinter(printer_id)))
        .into()
}

fn added_printer_row(printer: &PrinterEntry) -> Element<'static, Message> {
    let trailing = printer.web_page().map(|web_page| {
        button::custom(
            icon::from_name("view-web-browser-symbolic")
                .size(16)
                .icon()
                .class(cosmic::theme::Svg::Custom(primary_svg())),
        )
        .padding(8)
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .class(cosmic::theme::Button::Transparent)
        .on_press(Message::OpenPrinterWebPage(web_page.to_string()))
        .into()
    });

    container(two_line_printer_content(
        printer,
        fl!("printer-ready"),
        true,
        trailing,
    ))
    .padding([8, 24])
    .width(Length::Fill)
    .height(Length::Fixed(PRINTER_ROW_HEIGHT))
    .into()
}

fn two_line_printer_content(
    printer: &PrinterEntry,
    caption: String,
    checked: bool,
    trailing: Option<Element<'static, Message>>,
) -> Element<'static, Message> {
    let check: Element<'static, Message> = if checked {
        icon::from_name("checkbox-checked-symbolic")
            .size(16)
            .icon()
            .class(cosmic::theme::Svg::Custom(accent_svg()))
            .into()
    } else {
        horizontal_space().width(Length::Fixed(16.0)).into()
    };
    let copy = column::with_capacity(2)
        .spacing(0)
        .push(row_label(printer_display_name(printer)))
        .push(
            text::caption(caption)
                .class(cosmic::theme::Text::Color(TEXT_MUTED))
                .wrapping(Wrapping::None)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .width(Length::Fill)
                .height(Length::Fixed(17.0)),
        )
        .width(Length::Fill);
    let left = row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(8)
        .push(check)
        .push(copy)
        .width(Length::Fill);
    let mut content = row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(16)
        .push(left);
    if let Some(trailing) = trailing {
        content = content.push(trailing);
    }

    content.width(Length::Fill).into()
}

fn manual_application_row(application: &PrinterApplication) -> Element<'static, Message> {
    let application_id = application.id.clone();
    row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(16)
        .push(row_label(application_display_name(application)))
        .push(
            button::custom(centered_label(fl!("set-up-printer"), ACCENT))
                .padding(0)
                .width(Length::Fixed(122.0))
                .height(Length::Fixed(32.0))
                .class(cosmic::theme::Button::Transparent)
                .on_press(Message::OpenPrinterApplication(application_id)),
        )
        .padding([8, 24])
        .width(Length::Fill)
        .height(Length::Fixed(APPLICATION_ROW_HEIGHT))
        .into()
}

fn select_application_row(application: &PrinterApplication) -> Element<'static, Message> {
    let chevron: Element<'static, Message> = container(
        icon::from_name("go-next-symbolic")
            .size(16)
            .icon()
            .class(cosmic::theme::Svg::Custom(primary_svg())),
    )
    .width(Length::Fixed(32.0))
    .height(Length::Fixed(32.0))
    .center(Length::Fixed(32.0))
    .into();

    button::custom(
        row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(16)
            .push(row_label(application_display_name(application)))
            .push(chevron),
    )
    .padding([8, 24])
    .width(Length::Fill)
    .height(Length::Fixed(APPLICATION_ROW_HEIGHT))
    .class(cosmic::theme::Button::Transparent)
    .on_press(Message::SelectPrinterApplication(application.id.clone()))
    .into()
}

fn application_list(rows: Vec<Element<'static, Message>>) -> Element<'static, Message> {
    let row_count = rows.len().div_ceil(2).max(1);
    scrollable(list_view(rows))
        .direction(Direction::Vertical(Scrollbar::hidden()))
        .height(Length::Fixed(list_height(
            row_count,
            APPLICATION_ROW_HEIGHT,
            7,
        )))
        .into()
}

fn plain_row(label: String) -> Element<'static, Message> {
    container(row_label(label))
        .padding([0, 24])
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(Length::Fixed(APPLICATION_ROW_HEIGHT))
        .into()
}

fn section_heading(label: String) -> Element<'static, Message> {
    text::body(label)
        .font(cosmic::font::bold())
        .class(cosmic::theme::Text::Color(TEXT_MUTED))
        .width(Length::Fill)
        .height(Length::Fixed(21.0))
        .into()
}

fn regular_heading(label: String) -> Element<'static, Message> {
    text::body(label)
        .class(cosmic::theme::Text::Color(TEXT_MUTED))
        .width(Length::Fill)
        .height(Length::Fixed(21.0))
        .into()
}

fn row_label(label: String) -> Element<'static, Message> {
    text::body(label)
        .class(cosmic::theme::Text::Color(BODY_TEXT))
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
        .width(Length::Fill)
        .height(Length::Fixed(21.0))
        .into()
}

fn centered_label(label: String, color: Color) -> Element<'static, Message> {
    text::body(label)
        .class(cosmic::theme::Text::Color(color))
        .wrapping(Wrapping::None)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .into()
}

fn list_view(rows: Vec<Element<'static, Message>>) -> Element<'static, Message> {
    column::with_children(rows)
        .spacing(0)
        .width(Length::Fill)
        .apply(container)
        .width(Length::Fixed(CONTENT_WIDTH))
        .class(widgets::fill_container(DARK_LIST, RADIUS_CARD))
        .into()
}

fn with_dividers(rows: Vec<Element<'static, Message>>) -> Vec<Element<'static, Message>> {
    let mut divided = Vec::with_capacity(rows.len().saturating_mul(2).saturating_sub(1));
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            divided.push(widgets::divider());
        }
        divided.push(row);
    }
    divided
}

fn list_height(items: usize, row_height: f32, max_rows: usize) -> f32 {
    let visible = items.min(max_rows).max(1);
    visible as f32 * row_height + visible.saturating_sub(1) as f32
}

fn dialog_footer(label: String) -> Element<'static, Message> {
    let action = button::custom(centered_label(label, TEXT_MUTED))
        .padding([0, 16])
        .width(Length::Fixed(74.0))
        .height(Length::Fixed(32.0))
        .class(widgets::pill_button_style(BUTTON_CANCEL, TEXT_MUTED))
        .on_press(Message::Close);
    let footer_content = row::with_capacity(2)
        .push(horizontal_space())
        .push(action)
        .align_y(Alignment::Center)
        .width(Length::Fixed(640.0))
        .height(Length::Fixed(32.0))
        .apply(container)
        .padding([8, 12])
        .width(Length::Fixed(664.0))
        .height(Length::Fixed(48.0))
        .class(widgets::fill_container(DARK_FOOTER, RADIUS_CARD));

    container(footer_content)
        .padding(8)
        .width(Length::Fill)
        .height(Length::Fixed(64.0))
        .into()
}

fn printer_display_name(printer: &PrinterEntry) -> String {
    [
        printer.name(),
        printer.model().unwrap_or_default(),
        printer.device_uri().unwrap_or_default(),
        printer.id(),
    ]
    .into_iter()
    .find(|value| !value.is_empty())
    .map(str::to_string)
    .unwrap_or_else(|| fl!("generic-printer"))
}

fn application_display_name(application: &PrinterApplication) -> String {
    non_empty(&application.service_name)
        .map(str::to_string)
        .or_else(|| {
            application
                .make_and_model
                .as_deref()
                .and_then(non_empty)
                .map(str::to_string)
        })
        .unwrap_or_else(|| fl!("generic-printer-application"))
}

fn printer_location_label(printer: &PrinterEntry) -> String {
    printer
        .location()
        .and_then(non_empty)
        .map(str::to_string)
        .unwrap_or_else(|| fl!("printer-location-unknown"))
}

fn printer_matches_search(printer: &PrinterEntry, search: &str) -> bool {
    search.is_empty()
        || printer.name().to_lowercase().contains(search)
        || printer
            .model()
            .is_some_and(|value| value.to_lowercase().contains(search))
        || printer
            .location()
            .is_some_and(|value| value.to_lowercase().contains(search))
        || printer
            .device_uri()
            .is_some_and(|value| value.to_lowercase().contains(search))
}

fn application_matches_printer(application: &PrinterApplication, printer: &PrinterEntry) -> bool {
    let uri_endpoint = printer.device_uri().and_then(uri_endpoint);
    let Some(printer_port) = printer
        .port()
        .or_else(|| uri_endpoint.as_ref().map(|(_, port)| *port))
    else {
        return false;
    };
    if application.port != printer_port {
        return false;
    }

    let uri_host = uri_endpoint.as_ref().map(|(host, _)| host.as_str());
    let printer_hosts = printer
        .dnssd_address()
        .and_then(non_empty)
        .into_iter()
        .chain(printer.hostname().and_then(non_empty))
        .chain(uri_host);

    printer_hosts.into_iter().any(|printer_host| {
        let printer_host = normalize_host(printer_host);
        std::iter::once(application.hostname.as_str())
            .chain(application.addresses.iter().map(String::as_str))
            .any(|host| normalize_host(host) == printer_host)
    })
}

fn uri_endpoint(uri: &str) -> Option<(String, u16)> {
    let (scheme, rest) = uri.split_once("://")?;
    let authority = rest.split('/').next()?.trim_matches(['[', ']']);
    let default_port = match scheme {
        "ipp" | "ipps" => 631,
        "http" => 80,
        "https" => 443,
        _ => return None,
    };
    let (host, port) = authority
        .rsplit_once(':')
        .and_then(|(host, port)| port.parse().ok().map(|port| (host, port)))
        .unwrap_or((authority, default_port));
    Some((host.trim_matches(['[', ']']).to_string(), port))
}

fn normalize_host(host: &str) -> String {
    host.trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

fn discovered_queue_matches(configured: &PrinterEntry, discovered: &PrinterEntry) -> bool {
    if !discovered.id().is_empty() && queue_name(configured.id()) == discovered.id() {
        return true;
    }

    printer_device_uri(configured)
        .zip(printer_device_uri(discovered))
        .is_some_and(|(configured_uri, discovered_uri)| {
            normalized_uri(configured_uri) == normalized_uri(discovered_uri)
        })
}

fn queue_name(printer_id: &str) -> &str {
    printer_id
        .split_once('/')
        .map_or(printer_id, |(queue_name, _)| queue_name)
}

fn discovered_printer_id(printer: &PrinterEntry) -> String {
    let service_type = printer.option("dnssd-service-type").unwrap_or_default();
    let domain = printer.option("dnssd-domain").unwrap_or_default();
    let name = printer
        .option("dnssd-service-name")
        .unwrap_or_else(|| printer.name());

    format!("dnssd:{service_type}:{domain}:{name}")
}

fn printer_device_uri(printer: &PrinterEntry) -> Option<&str> {
    printer.device_uri().or_else(|| printer.printer_local_uri())
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized_uri(uri: &str) -> String {
    uri.split(['?', '#'])
        .next()
        .unwrap_or(uri)
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn dialog_container() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::widget::container::Style {
        background: Some(Background::Color(DARK_DIALOG)),
        border: Border {
            color: BORDER_SUBTLE,
            radius: RADIUS_CARD.into(),
            width: 1.0,
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.32),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 16.0,
        },
        ..Default::default()
    })
}

fn primary_svg() -> Rc<dyn Fn(&cosmic::Theme) -> cosmic::widget::svg::Style> {
    Rc::new(|_theme: &cosmic::Theme| cosmic::widget::svg::Style {
        color: Some(BODY_TEXT),
    })
}

fn accent_svg() -> Rc<dyn Fn(&cosmic::Theme) -> cosmic::widget::svg::Style> {
    Rc::new(|_theme: &cosmic::Theme| cosmic::widget::svg::Style {
        color: Some(ACCENT),
    })
}
