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
use cosmic_settings_printers_core::{
    AddPrinterDiscoveryReply, AddPrinterDiscoveryState, ConfigureDiscoveredPrinterRequest,
    ConfigurePrinterReply, DiscoveredPhysicalPrinter, DiscoveryGeneration, Error as PrinterError,
    ManualSetupPrinterApplication, PaCandidateState, PrinterApplicationCandidateSummary,
    PrinterConfigurationState, PrinterEntry,
};

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
    SelectApplication { printer_id: String },
    ManualSetup,
    Adding { printer_id: String },
    Added,
}

#[derive(Clone, Debug)]
pub struct Page {
    pub search: String,
    pub error: Option<String>,
    pub configured_printers: Vec<PrinterEntry>,
    pub view: DialogView,
    discovery: Option<AddPrinterDiscoveryReply>,
    manual_setup_applications: Vec<ManualSetupPrinterApplication>,
    pending_operation: Option<String>,
    added: Vec<AddedPrinter>,
}

#[derive(Clone, Debug)]
struct AddedPrinter {
    name: String,
    destination_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Load {
    discovery: AddPrinterDiscoveryReply,
    manual_setup_applications: Vec<ManualSetupPrinterApplication>,
}

impl Page {
    pub fn new(configured_printers: Vec<PrinterEntry>) -> Self {
        Self {
            search: String::new(),
            error: None,
            configured_printers,
            view: DialogView::Discovery,
            discovery: None,
            manual_setup_applications: Vec::new(),
            pending_operation: None,
            added: Vec::new(),
        }
    }

    /// Starts a fresh round of discovery and loads its first results.
    pub fn load_task() -> Task<crate::Message> {
        loaded_task(start_discovery())
    }

    /// Reloads the results of the round already in progress.
    pub fn refresh_task() -> Task<crate::Message> {
        loaded_task(load())
    }

    pub fn update(&mut self, message: Message) -> Action {
        match message {
            Message::Close => Action::Close,
            Message::Search(search) => {
                self.search = search;
                Action::None
            }
            Message::Loaded(result) => {
                self.apply_load(result);
                Action::None
            }
            Message::ConfigurationChanged => self.poll_configuration(),
            Message::OpenManualSetup => {
                self.open_manual_setup();
                Action::None
            }
            Message::SelectDiscoveredPrinter(printer_id) => {
                self.select_discovered_printer(printer_id)
            }
            Message::SelectPrinterApplication(candidate_id) => {
                self.select_printer_application(candidate_id)
            }
            Message::OpenPrinterWebPage(web_page) => Self::open_web_page(web_page),
            Message::PrinterSetupFinished(result) => self.finish_printer_setup(result),
            Message::WebPageOpened(result) => self.finish_web_page_open(result),
        }
    }

    fn apply_load(&mut self, result: Result<Load, String>) {
        match result {
            Ok(load) => {
                self.error = None;
                self.discovery = Some(load.discovery);
                self.manual_setup_applications = load.manual_setup_applications;
                // Nothing was found, so there is nothing to choose from and the
                // wizard opens where the user can still get somewhere. This asks
                // what discovery found, not what the search box is showing: a
                // search term that matches nothing is not an empty result.
                if self.view == DialogView::Discovery
                    && !self.is_searching()
                    && self.printers_to_set_up().next().is_none()
                {
                    self.view = DialogView::ManualSetup;
                }
            }
            Err(why) => {
                tracing::error!(why, "failed to load add printer discovery");
                self.error = Some(fl!("failed-to-load-printers"));
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
        if self.is_searching() || self.is_adding() {
            return Action::None;
        }

        let Some(printer) = self.physical_printer(&printer_id) else {
            self.error = Some(fl!("no-printers-found"));
            return Action::None;
        };

        match selectable_candidate_ids(printer).as_slice() {
            // Nothing can drive it, so the way forward is a Printer Application's
            // own setup rather than an error the user cannot act on.
            [] => {
                self.open_manual_setup();
                Action::None
            }
            [candidate_id] => {
                let candidate_id = candidate_id.clone();
                self.start_setup(printer_id, candidate_id)
            }
            _ => {
                self.error = None;
                self.view = DialogView::SelectApplication { printer_id };
                Action::None
            }
        }
    }

    fn select_printer_application(&mut self, candidate_id: String) -> Action {
        let DialogView::SelectApplication { printer_id } = &self.view else {
            return Action::None;
        };
        let printer_id = printer_id.clone();

        let offered = self
            .physical_printer(&printer_id)
            .is_some_and(|printer| selectable_candidate_ids(printer).contains(&candidate_id));

        if offered {
            self.start_setup(printer_id, candidate_id)
        } else {
            Action::None
        }
    }

    fn start_setup(&mut self, printer_id: String, candidate_id: String) -> Action {
        if self.is_adding() {
            return Action::None;
        }

        // Rows left over from an earlier round describe devices that may be gone,
        // so a fresh round is started instead of acting on them.
        let Some(discovery_generation) = self.selectable_generation() else {
            self.view = DialogView::Discovery;
            return Action::Task(Self::load_task());
        };

        self.error = None;
        self.view = DialogView::Adding {
            printer_id: printer_id.clone(),
        };

        Action::Task(setup_task(ConfigureDiscoveredPrinterRequest {
            discovery_generation,
            physical_printer_id: printer_id,
            candidate_id,
            requested_display_name: None,
        }))
    }

    fn poll_configuration(&self) -> Action {
        let Some(operation_id) = self.pending_operation.clone() else {
            return Action::None;
        };

        Action::Task(cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::PrinterSetupFinished(printer_configuration(operation_id).await).into(),
            ))
        }))
    }

    fn finish_printer_setup(
        &mut self,
        result: Result<ConfigurePrinterReply, SetupError>,
    ) -> Action {
        self.pending_operation = None;

        match result {
            Ok(reply) => self.apply_configuration(reply),
            Err(SetupError::ManualSetup { web_interface_uri }) => {
                self.error = None;
                self.continue_in_printer_application(web_interface_uri)
            }
            Err(SetupError::Failed(why)) => {
                tracing::error!(why, "failed to configure discovered printer");
                self.error = Some(why);
                self.view = DialogView::Discovery;
                Action::None
            }
        }
    }

    fn apply_configuration(&mut self, reply: ConfigurePrinterReply) -> Action {
        match reply.state {
            PrinterConfigurationState::Creating
            | PrinterConfigurationState::AwaitingAdvertisement => {
                self.pending_operation = Some(reply.operation_id);
                // Refreshing destinations is what lets the daemon match the new
                // printer to the queue it eventually advertises.
                Action::RefreshPrinters
            }
            PrinterConfigurationState::Reconciled
            | PrinterConfigurationState::AlreadyConfigured => {
                self.error = None;
                self.added.push(AddedPrinter {
                    name: reply.configured_printer_name,
                    destination_id: reply.destination_id,
                });
                self.view = DialogView::Added;
                // The printer that was just set up has to stop being offered, and
                // only a fresh round can establish that.
                Action::RediscoverPrinters
            }
            PrinterConfigurationState::ManualActionRequired => {
                self.continue_in_printer_application(reply.web_interface_uri)
            }
            PrinterConfigurationState::UnknownOutcome | PrinterConfigurationState::Failed => {
                self.error = Some(fl!("failed-to-add-printer"));
                self.view = DialogView::Discovery;
                Action::None
            }
        }
    }

    fn continue_in_printer_application(&mut self, web_interface_uri: Option<String>) -> Action {
        self.view = DialogView::ManualSetup;

        match web_interface_uri {
            Some(web_page) => Self::open_web_page(web_page),
            None => {
                self.error = Some(fl!("printer-application-web-interface-unavailable"));
                Action::None
            }
        }
    }

    /// Reports a page that could not be opened by naming it.
    ///
    /// A session with no browser handler is not a reason to strand the user: the
    /// address is the whole of what they need, so it is shown rather than the
    /// failure of the tool that would have opened it.
    fn finish_web_page_open(&mut self, result: Result<(), (String, String)>) -> Action {
        if let Err((address, why)) = result {
            tracing::error!(why, address, "failed to open printer web page");
            self.error = Some(fl!(
                "open-printer-application-page-manually",
                address = address
            ));
        }

        Action::None
    }

    fn visible_printers(&self) -> impl Iterator<Item = &DiscoveredPhysicalPrinter> {
        let search = self.search.trim().to_lowercase();

        self.printers_to_set_up()
            .filter(move |printer| printer_matches_search(printer, &search))
    }

    /// The printers still worth offering.
    ///
    /// A printer a Printer Application has already set up is left out: it is not
    /// something to add, and adding it again would produce a second queue for one
    /// printer.
    fn printers_to_set_up(&self) -> impl Iterator<Item = &DiscoveredPhysicalPrinter> {
        self.physical_printers()
            .iter()
            .filter(|printer| !is_configured(printer))
    }

    fn physical_printers(&self) -> &[DiscoveredPhysicalPrinter] {
        self.discovery
            .as_ref()
            .map_or(&[], |discovery| discovery.physical_printers.as_slice())
    }

    fn physical_printer(&self, printer_id: &str) -> Option<&DiscoveredPhysicalPrinter> {
        self.physical_printers()
            .iter()
            .find(|printer| printer.id == printer_id)
    }

    fn configured_printer(&self, destination_id: &str) -> Option<&PrinterEntry> {
        self.configured_printers
            .iter()
            .find(|printer| printer.id() == destination_id)
    }

    fn selectable_generation(&self) -> Option<DiscoveryGeneration> {
        self.discovery
            .as_ref()
            .filter(|discovery| !discovery.cached)
            .map(|discovery| discovery.generation)
    }

    /// Whether every Printer Application in this round has answered.
    ///
    /// Rows appear as each application answers, so a row can be complete before the
    /// round is. Nothing may be concluded from the absence of a candidate until this
    /// is true.
    fn every_application_answered(&self) -> bool {
        self.discovery.as_ref().is_some_and(|discovery| {
            discovery.completed_printer_application_scans
                >= discovery.total_printer_application_scans
        })
    }

    fn is_searching(&self) -> bool {
        self.error.is_none()
            && self
                .discovery
                .as_ref()
                .is_none_or(|discovery| discovery.state == AddPrinterDiscoveryState::Searching)
    }

    fn is_adding(&self) -> bool {
        matches!(self.view, DialogView::Adding { .. })
    }

    fn open_web_page(web_page: String) -> Action {
        Action::Task(cosmic::task::future(async move {
            let opened = backend::open_printer_web_page(web_page.clone())
                .await
                .map_err(|why| (web_page, why));

            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::WebPageOpened(opened).into(),
            ))
        }))
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    Close,
    Search(String),
    Loaded(Result<Load, String>),
    ConfigurationChanged,
    OpenManualSetup,
    SelectDiscoveredPrinter(String),
    SelectPrinterApplication(String),
    OpenPrinterWebPage(String),
    PrinterSetupFinished(Result<ConfigurePrinterReply, SetupError>),
    /// The address is carried back so a failure can name the page the user still
    /// needs to reach.
    WebPageOpened(Result<(), (String, String)>),
}

/// Why a configuration attempt could not complete here.
///
/// A Printer Application that cannot create the printer itself offers its own
/// page instead, which is a different outcome from an outright failure.
#[derive(Clone, Debug)]
pub enum SetupError {
    ManualSetup { web_interface_uri: Option<String> },
    Failed(String),
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
    /// Reload the printers and start a fresh round of discovery.
    RediscoverPrinters,
    Task(Task<crate::Message>),
}

fn loaded_task(
    load: impl Future<Output = Result<Load, String>> + Send + 'static,
) -> Task<crate::Message> {
    cosmic::task::future(async move {
        crate::Message::PageMessage(crate::pages::Message::Printers(
            Message::Loaded(load.await).into(),
        ))
    })
}

fn setup_task(request: ConfigureDiscoveredPrinterRequest) -> Task<crate::Message> {
    cosmic::task::future(async move {
        crate::Message::PageMessage(crate::pages::Message::Printers(
            Message::PrinterSetupFinished(configure(request).await).into(),
        ))
    })
}

async fn start_discovery() -> Result<Load, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    client
        .start_add_printer_discovery()
        .await
        .map_err(|why| why.to_string())?;

    discovery_load(&mut client).await
}

async fn load() -> Result<Load, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;

    discovery_load(&mut client).await
}

async fn discovery_load(client: &mut printers_client::Client) -> Result<Load, String> {
    let discovery = client
        .add_printer_discovery()
        .await
        .map_err(|why| why.to_string())?;
    let manual_setup_applications = client
        .manual_setup_printer_applications()
        .await
        .map_err(|why| why.to_string())?;

    Ok(Load {
        discovery,
        manual_setup_applications,
    })
}

async fn configure(
    request: ConfigureDiscoveredPrinterRequest,
) -> Result<ConfigurePrinterReply, SetupError> {
    let mut client = printers_client::connect().await.map_err(setup_error)?;

    client
        .configure_discovered_printer(request)
        .await
        .map_err(setup_error)
}

async fn printer_configuration(operation_id: String) -> Result<ConfigurePrinterReply, SetupError> {
    let mut client = printers_client::connect().await.map_err(setup_error)?;

    client
        .printer_configuration(&operation_id)
        .await
        .map_err(setup_error)
}

fn setup_error(error: printers_client::ClientError) -> SetupError {
    match error {
        printers_client::ClientError::Service(
            PrinterError::PrinterConfigurationManualActionRequired {
                web_interface_uri, ..
            },
        ) => SetupError::ManualSetup { web_interface_uri },
        error => SetupError::Failed(error.to_string()),
    }
}

pub fn dialog(page: &Page) -> Element<'_, crate::pages::Message> {
    let body = match &page.view {
        DialogView::Discovery | DialogView::Adding { .. } => discovery_view(page),
        DialogView::ManualSetup => manual_setup_view(page),
        DialogView::SelectApplication { printer_id } => select_application_view(page, printer_id),
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
    if !page.is_searching() {
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
    let mut rows = Vec::with_capacity(page.manual_setup_applications.len().max(1) + 1);
    if let Some(error) = &page.error {
        rows.push(plain_row(error.clone()));
    }
    rows.extend(
        page.manual_setup_applications
            .iter()
            .map(manual_application_row),
    );
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

fn select_application_view<'a>(page: &'a Page, printer_id: &str) -> Element<'a, Message> {
    let rows = page
        .physical_printer(printer_id)
        .map(|printer| {
            with_dividers(
                printer
                    .candidates
                    .iter()
                    .filter(|candidate| is_selectable(candidate.state))
                    .map(select_application_row)
                    .collect(),
            )
        })
        .unwrap_or_default();

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
    let rows = with_dividers(
        page.added
            .iter()
            .map(|added| added_printer_row(page, added))
            .collect(),
    );
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
    let (rows, printer_count, row_height) = if page.is_searching() {
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

fn discovered_printer_row(
    page: &Page,
    printer: &DiscoveredPhysicalPrinter,
) -> Element<'static, Message> {
    let printer_id = printer.id.clone();
    let connecting = matches!(
        &page.view,
        DialogView::Adding { printer_id: adding_id } if adding_id == &printer_id
    );
    let status = if connecting {
        fl!("connecting")
    } else {
        candidate_summary(printer, page.every_application_answered())
    };
    let content = two_line_printer_content(printer.display_name.clone(), status, connecting, None);

    button::custom(content)
        .padding([8, 24])
        .width(Length::Fill)
        .height(Length::Fixed(PRINTER_ROW_HEIGHT))
        .class(cosmic::theme::Button::Transparent)
        .on_press_maybe((!page.is_adding()).then_some(Message::SelectDiscoveredPrinter(printer_id)))
        .into()
}

fn added_printer_row(page: &Page, added: &AddedPrinter) -> Element<'static, Message> {
    let destination = added
        .destination_id
        .as_deref()
        .and_then(|destination_id| page.configured_printer(destination_id));
    let name = destination.map_or_else(|| added.name.clone(), printer_display_name);
    let trailing = destination
        .and_then(PrinterEntry::web_page)
        .map(|web_page| {
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
        name,
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
    name: String,
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
        .push(row_label(name))
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

fn manual_application_row(
    application: &ManualSetupPrinterApplication,
) -> Element<'static, Message> {
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
                .on_press(Message::OpenPrinterWebPage(
                    application.web_interface_uri.clone(),
                )),
        )
        .padding([8, 24])
        .width(Length::Fill)
        .height(Length::Fixed(APPLICATION_ROW_HEIGHT))
        .into()
}

fn select_application_row(
    candidate: &PrinterApplicationCandidateSummary,
) -> Element<'static, Message> {
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
            .push(row_label(candidate.printer_application_name.clone()))
            .push(chevron),
    )
    .padding([8, 24])
    .width(Length::Fill)
    .height(Length::Fixed(APPLICATION_ROW_HEIGHT))
    .class(cosmic::theme::Button::Transparent)
    .on_press(Message::SelectPrinterApplication(candidate.id.clone()))
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

fn application_display_name(application: &ManualSetupPrinterApplication) -> String {
    non_empty(&application.display_name)
        .map(str::to_string)
        .unwrap_or_else(|| fl!("generic-printer-application"))
}

/// Names the Printer Applications offering to set this printer up.
///
/// A Printer Application reports every printer it can see, whether or not it has
/// a driver for it, so a row can be real and still have nobody to drive it. Saying
/// how many applications saw it reads as the truth rather than as a contradiction
/// of the row being there at all.
///
/// Until every application has answered, saying nothing can drive this printer
/// would be asserting something not yet known — the application with the driver may
/// be the one still being asked. So while a round is running the row says it is
/// still looking.
fn candidate_summary(printer: &DiscoveredPhysicalPrinter, every_answer_in: bool) -> String {
    let names = printer
        .candidates
        .iter()
        .filter(|candidate| is_selectable(candidate.state))
        .map(|candidate| candidate.printer_application_name.as_str())
        .collect::<Vec<_>>();

    if !names.is_empty() {
        return names.join(", ");
    }
    if !every_answer_in {
        return fl!("searching");
    }

    match printer.candidates.len() {
        0 => fl!("no-compatible-printer-applications"),
        count => fl!("seen-without-a-driver", count = count),
    }
}

/// Whether a Printer Application can be chosen to set this printer up.
fn is_selectable(state: PaCandidateState) -> bool {
    state == PaCandidateState::Ready
}

/// Whether a Printer Application has already set this printer up.
fn is_configured(printer: &DiscoveredPhysicalPrinter) -> bool {
    printer
        .candidates
        .iter()
        .any(|candidate| candidate.state == PaCandidateState::AlreadyConfigured)
}

fn selectable_candidate_ids(printer: &DiscoveredPhysicalPrinter) -> Vec<String> {
    printer
        .candidates
        .iter()
        .filter(|candidate| is_selectable(candidate.state))
        .map(|candidate| candidate.id.clone())
        .collect()
}

fn printer_matches_search(printer: &DiscoveredPhysicalPrinter, search: &str) -> bool {
    search.is_empty()
        || printer.display_name.to_lowercase().contains(search)
        || printer
            .make_and_model
            .as_deref()
            .is_some_and(|value| value.to_lowercase().contains(search))
        || printer.candidates.iter().any(|candidate| {
            candidate
                .printer_application_name
                .to_lowercase()
                .contains(search)
        })
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
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
