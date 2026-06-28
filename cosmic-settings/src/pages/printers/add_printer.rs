use cosmic::app::Task;
use cosmic::iced::{Alignment, Length};
use cosmic::iced_core::text::Wrapping;
use cosmic::widget::{
    self, button, column, container, icon, row, settings, space::horizontal as horizontal_space,
    text,
};
use cosmic::{Apply, Element};
use cosmic_settings_printers_client::{self as printers_client, CosmicPrintersProxy};
use cosmic_settings_printers_core::{PrinterEntry, printers_match};

#[derive(Clone, Debug)]
pub struct Page {
    pub search: String,
    pub loading: bool,
    pub error: Option<String>,
    pub configured_printers: Vec<PrinterEntry>,
    pub discovered_printers: Vec<PrinterEntry>,
    pub selection: Option<Selection>,
    pub adding: bool,
}

impl Page {
    pub fn new(configured_printers: Vec<PrinterEntry>) -> Self {
        Self {
            search: String::new(),
            loading: true,
            error: None,
            configured_printers,
            discovered_printers: Vec::new(),
            selection: None,
            adding: false,
        }
    }

    pub fn filtered_printers(&self) -> impl Iterator<Item = &PrinterEntry> {
        let search = self.search.trim().to_lowercase();

        self.discovered_printers.iter().filter(move |printer| {
            search.is_empty()
                || printer.name.to_lowercase().contains(&search)
                || printer.model.to_lowercase().contains(&search)
                || printer.location.to_lowercase().contains(&search)
                || printer.device_uri.to_lowercase().contains(&search)
        })
    }

    pub fn selected_discovered_id(&self) -> Option<&str> {
        match &self.selection {
            Some(Selection::DiscoveredPrinter(id)) => Some(id.as_str()),
            Some(Selection::Manual(_)) | None => None,
        }
    }

    pub fn selected_manual_action(&self) -> Option<ManualAddAction> {
        match self.selection {
            Some(Selection::Manual(action)) => Some(action),
            Some(Selection::DiscoveredPrinter(_)) | None => None,
        }
    }

    pub fn can_confirm(&self) -> bool {
        self.selected_discovered_id()
            .is_some_and(|printer_id| !self.printer_is_configured_id(printer_id))
            && !self.loading
            && !self.adding
            && self.error.is_none()
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
            Message::DiscoveredPrintersLoaded(Ok(printers)) => {
                self.loading = false;
                self.error = None;
                self.discovered_printers = printers;
                Action::None
            }
            Message::DiscoveredPrintersLoaded(Err(why)) => {
                tracing::error!(why, "failed to discover printers");
                self.loading = false;
                self.error = Some(fl!("failed-to-load-printers"));
                self.discovered_printers.clear();
                self.selection = None;
                Action::None
            }
            Message::SelectDiscoveredPrinter(id) => {
                self.selection = Some(Selection::DiscoveredPrinter(id.clone()));

                if self.loading
                    || self.adding
                    || self.error.is_some()
                    || self.printer_is_configured_id(&id)
                {
                    return Action::None;
                }

                self.start_add_discovered_printer(id)
            }
            Message::SelectManualAddAction(action) => {
                // TODO: Wire manual add actions once the printer backend exposes flows
                // for IP-address and generic network-printer setup.
                self.selection = Some(Selection::Manual(action));
                Action::None
            }
            Message::Confirm => {
                let Some(Selection::DiscoveredPrinter(printer_id)) = self.selection.clone() else {
                    return Action::None;
                };

                if self.adding {
                    return Action::None;
                }

                if self.printer_is_configured_id(&printer_id) {
                    return Action::None;
                }

                self.start_add_discovered_printer(printer_id)
            }
            Message::DiscoveredPrinterAdded(Ok(())) => {
                self.adding = false;
                self.mark_selected_printer_configured();
                Action::RefreshPrinters
            }
            Message::DiscoveredPrinterAdded(Err(why)) => {
                tracing::error!(why, "failed to add discovered printer");
                self.adding = false;
                self.error = Some(why);
                Action::None
            }
        }
    }

    fn printer_is_configured_id(&self, printer_id: &str) -> bool {
        self.discovered_printers
            .iter()
            .find(|printer| printer.id == printer_id)
            .is_some_and(|printer| self.printer_is_configured(printer))
    }

    fn printer_is_configured(&self, printer: &PrinterEntry) -> bool {
        self.configured_printers
            .iter()
            .any(|configured| printers_match(configured, printer))
    }

    fn mark_selected_printer_configured(&mut self) {
        let Some(printer_id) = self.selected_discovered_id() else {
            return;
        };

        let Some(printer) = self
            .discovered_printers
            .iter()
            .find(|printer| printer.id == printer_id)
            .cloned()
        else {
            return;
        };

        if !self.printer_is_configured(&printer) {
            self.configured_printers.push(printer);
        }
    }

    fn start_add_discovered_printer(&mut self, printer_id: String) -> Action {
        self.adding = true;
        self.error = None;

        Action::Task(cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::Printers(
                Message::DiscoveredPrinterAdded(add_discovered_printer(printer_id).await).into(),
            ))
        }))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selection {
    DiscoveredPrinter(String),
    Manual(ManualAddAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualAddAction {
    UseIpAddress,
    NetworkPrinter,
}

#[derive(Clone, Debug)]
pub enum Message {
    Close,
    Search(String),
    DiscoveredPrintersLoaded(Result<Vec<PrinterEntry>, String>),
    SelectDiscoveredPrinter(String),
    SelectManualAddAction(ManualAddAction),
    Confirm,
    DiscoveredPrinterAdded(Result<(), String>),
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
    let reply = client
        .conn
        .list_discovered_printers()
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))?;

    Ok(reply.printers)
}

async fn add_discovered_printer(printer_id: String) -> Result<(), String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;

    client
        .conn
        .add_discovered_printer(printer_id)
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))
}

pub fn dialog(page: &Page) -> Element<'_, crate::pages::Message> {
    let spacing = cosmic::theme::spacing();

    let search = widget::search_input(fl!("type-to-search"), &page.search)
        .on_input(Message::Search)
        .on_clear(Message::Search(String::new()))
        .apply(container)
        .width(Length::Fixed(314.0))
        .center_x(Length::Fill);

    let content = column::with_capacity(3)
        .spacing(spacing.space_m)
        .push(search)
        .push(printers_section(page))
        .push(manual_section(page))
        .width(Length::Fixed(555.0));

    let content_area = content
        .apply(container)
        .padding([spacing.space_l, 0])
        .center_x(Length::Fill)
        .height(Length::Fixed(506.0))
        .width(Length::Fill);

    let next = button::suggested(fl!("next"))
        .on_press_maybe(page.can_confirm().then_some(Message::Confirm));

    let footer_buttons = row::with_capacity(2)
        .spacing(spacing.space_xxs)
        .align_y(Alignment::Center)
        .push(button::standard(fl!("cancel")).on_press(Message::Close))
        .push(next);

    let footer_bar = row::with_capacity(2)
        .push(horizontal_space())
        .push(footer_buttons)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .apply(container)
        .padding([spacing.space_xxs, spacing.space_xs])
        .height(Length::Fixed(48.0))
        .width(Length::Fill)
        .class(cosmic::theme::Container::Primary);

    let footer = footer_bar
        .apply(container)
        .padding(spacing.space_xxs)
        .height(Length::Fixed(64.0))
        .width(Length::Fill);

    column::with_capacity(2)
        .push(content_area)
        .push(footer)
        .width(Length::Fixed(680.0))
        .height(Length::Fixed(570.0))
        .apply(container)
        .width(Length::Fixed(680.0))
        .height(Length::Fixed(570.0))
        .class(cosmic::theme::Container::Dialog)
        .apply(Element::from)
        .map(super::Message::from)
        .map(crate::pages::Message::Printers)
}

fn printers_section(page: &Page) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();

    let mut list = widget::list_column();

    if page.loading {
        list = list.add(plain_row(fl!("loading")));
    } else if let Some(error) = &page.error {
        list = list.add(plain_row(error.clone()));
    } else {
        let mut found = false;

        for printer in page.filtered_printers() {
            found = true;
            let configured = page.printer_is_configured(printer);
            let selected = page.selected_discovered_id() == Some(printer.id.as_str());
            list = list.add(discovered_printer_row(
                printer,
                selected,
                configured,
                page.adding && selected,
            ));
        }

        if !found {
            list = list.add(plain_row(fl!("no-printers-found")));
        }
    }

    column::with_capacity(2)
        .spacing(spacing.space_xxs)
        .push(text::body(fl!("printers")).font(cosmic::font::bold()))
        .push(list)
        .into()
}

fn manual_section(page: &Page) -> Element<'_, Message> {
    let spacing = cosmic::theme::spacing();

    let list = widget::list_column()
        .add(manual_row(
            fl!("use-ip-address"),
            ManualAddAction::UseIpAddress,
            page.selected_manual_action() == Some(ManualAddAction::UseIpAddress),
        ))
        .add(manual_row(
            fl!("network-printer"),
            ManualAddAction::NetworkPrinter,
            page.selected_manual_action() == Some(ManualAddAction::NetworkPrinter),
        ));

    column::with_capacity(2)
        .spacing(spacing.space_xxs)
        .push(text::body(fl!("cant-find-printer")).font(cosmic::font::bold()))
        .push(list)
        .into()
}

fn plain_row(label: String) -> Element<'static, Message> {
    settings::item_row(vec![
        text::body(label)
            .wrapping(Wrapping::Word)
            .width(Length::Fill)
            .into(),
    ])
    .into()
}

fn discovered_printer_row(
    printer: &PrinterEntry,
    selected: bool,
    configured: bool,
    connecting: bool,
) -> Element<'static, Message> {
    let spacing = cosmic::theme::spacing();

    let check: Element<'_, Message> = if selected || configured {
        icon::from_name("object-select-symbolic")
            .size(16)
            .icon()
            .into()
    } else {
        horizontal_space().width(Length::Fixed(16.0)).into()
    };

    let mut copy = column::with_capacity(2)
        .push(text::body(printer.name.clone()).wrapping(Wrapping::Word))
        .spacing(0);

    if selected || configured {
        let status = if connecting {
            fl!("connecting")
        } else {
            fl!("printer-ready")
        };
        copy = copy.push(text::caption(status));
    }

    let row = settings::item_row(vec![
        check,
        copy.width(Length::Fill).into(),
        icon::from_name("web-settings-symbolic")
            .size(16)
            .icon()
            .into(),
    ])
    .spacing(spacing.space_xxs)
    .apply(container)
    .class(cosmic::theme::Container::List)
    .apply(button::custom)
    .padding(0)
    .class(cosmic::theme::Button::Transparent)
    .on_press(Message::SelectDiscoveredPrinter(printer.id.clone()));

    row.into()
}

fn manual_row(label: String, action: ManualAddAction, selected: bool) -> Element<'static, Message> {
    let label = text::body(label)
        .class(if selected {
            cosmic::theme::Text::Accent
        } else {
            cosmic::theme::Text::Default
        })
        .wrapping(Wrapping::Word)
        .width(Length::Fill);

    settings::item_row(vec![
        label.into(),
        horizontal_space().into(),
        row::with_capacity(1)
            .push(icon::from_name("go-next-symbolic").size(16).icon())
            .align_y(Alignment::Center)
            .into(),
    ])
    .apply(container)
    .class(cosmic::theme::Container::List)
    .apply(button::custom)
    .padding(0)
    .class(cosmic::theme::Button::Transparent)
    .on_press(Message::SelectManualAddAction(action))
    .into()
}
