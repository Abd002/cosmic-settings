use cosmic::app::Task;
use cosmic::iced::border::Radius;
use cosmic::iced::{Alignment, Color, Length, Subscription, event, keyboard};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{
    self, button, column, container, row, space::horizontal as horizontal_space, text,
};
use cosmic::{Apply, Element};
use cosmic_settings_page::{self as page, Section, section};
use cosmic_settings_printers_core::{PrinterStatus, SupplyLevel, SupplyRgb, SupplyWarning};
use slotmap::SlotMap;

use super::style::{
    ACCENT, BODY_TEXT, CARD_BG, DIVIDER, FONT_SEMIBOLD, RADIUS_CARD, RADIUS_PILL,
    RADIUS_SUPPLY_BAR, REMOVE_BG, REMOVE_TEXT, SECONDARY_TEXT, STATUS_READY, STATUS_STOPPED,
    SUPPLY_BAR_HEIGHT, SUPPLY_CARD_PADDING_Y, SUPPLY_COLUMN_SPACING, SUPPLY_DOT_SIZE,
    SUPPLY_GRAPH_HEIGHT, SUPPLY_LABEL_HEIGHT, SUPPLY_MARK_HEIGHT, SUPPLY_MARK_WIDTH,
    SUPPLY_MIN_CHANNEL, SUPPLY_NEUTRAL, SUPPLY_OUTLINE_TOLERANCE, SUPPLY_PERCENTAGE_WIDTH,
    SUPPLY_ROW_SPACING, SUPPLY_TRACK, SUPPLY_TRACK_HEIGHT, TITLE_TEXT,
};
use super::{PrinterEntry, backend, widgets};

const ROW_HEIGHT: f32 = 48.0;

#[derive(Clone, Debug)]
pub enum Message {
    GoBack,
    EditLocation(String),
    EditLocationChanged(String),
    SubmitLocation(String, String),
    CancelDialog,
    LoadPrinter {
        printer: PrinterEntry,
        is_default: bool,
        parent_page: page::Entity,
        queue_page: page::Entity,
        available_printers: Vec<PrinterEntry>,
    },
    OpenPrinterQueue(String),
    RemovePrinter(String),
    PrinterDeleted(Result<(), String>),
    SelectPaperSize(String, usize),
    SelectPrintSides(String, usize),
    PrinterOptionDefaultSet(Result<(), String>),
    TogglePaperSizeDropdown(bool),
    TogglePrintSidesDropdown(bool),
    ToggleDefaultPrinter(String, bool),
    PrinterDefaultSet(Result<(), String>),
    PrinterLocationSet(Result<(), String>),
    SuppliesLoaded {
        printer_id: String,
        result: Result<Vec<SupplyLevel>, String>,
    },
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
    parent_page: page::Entity,
    queue_page: page::Entity,
    printer: Option<PrinterEntry>,
    available_printers: Vec<PrinterEntry>,
    dialog: Option<Dialog>,
    is_default: bool,
    paper_size_dropdown_open: bool,
    print_sides_dropdown_open: bool,
    /// What the printer last said it holds. Empty until it answers, and for a printer
    /// that reports nothing — which is most of them.
    supplies: Vec<SupplyLevel>,
}

#[derive(Clone, Debug)]
enum Dialog {
    EditLocation {
        printer_id: String,
        location: String,
    },
}

impl Default for Page {
    fn default() -> Self {
        Self {
            entity: page::Entity::default(),
            parent_page: page::Entity::default(),
            queue_page: page::Entity::default(),
            printer: None,
            available_printers: Vec::new(),
            dialog: None,
            is_default: false,
            paper_size_dropdown_open: false,
            print_sides_dropdown_open: false,
            supplies: Vec::new(),
        }
    }
}

impl page::AutoBind<crate::pages::Message> for Page {}

impl page::Page<crate::pages::Message> for Page {
    fn set_id(&mut self, entity: page::Entity) {
        self.entity = entity;
    }

    fn info(&self) -> page::Info {
        page::Info::new("printer-details", "printer-symbolic")
            .title(fl!("printer-details"))
            .description(fl!("printer-details-description"))
    }

    fn subscription(&self, _core: &cosmic::Core) -> Subscription<crate::pages::Message> {
        event::listen_with(|event, _, _| match event {
            cosmic::iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(crate::pages::Message::PrinterQueue(
                    super::queue::Message::ModifiersChanged(modifiers),
                ))
            }
            _ => None,
        })
    }

    fn header(&self) -> Option<Element<'_, crate::pages::Message>> {
        self.printer
            .as_ref()
            .map(details_header)
            .map(Element::from)
            .map(|element| element.map(crate::pages::Message::PrinterDetails))
    }

    fn content(
        &self,
        sections: &mut SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        Some(vec![sections.insert(Section::default().view::<Page>(
            move |_binder, page, _section| {
                let Some(printer) = page.printer.as_ref() else {
                    return empty_state();
                };

                view_details(page, printer, page.is_default)
            },
        ))])
    }

    fn dialog(&self) -> Option<Element<'_, crate::pages::Message>> {
        self.dialog.as_ref().map(|dialog| match dialog {
            Dialog::EditLocation {
                printer_id,
                location,
            } => {
                let input = widget::text_input("", location)
                    .on_input(Message::EditLocationChanged)
                    .on_submit({
                        let printer_id = printer_id.clone();
                        move |location| Message::SubmitLocation(printer_id.clone(), location)
                    });

                let primary_action = widget::button::suggested(fl!("save")).on_press(
                    Message::SubmitLocation(printer_id.clone(), location.clone()),
                );
                let secondary_action =
                    widget::button::standard(fl!("cancel")).on_press(Message::CancelDialog);

                widget::dialog()
                    .title(fl!("location"))
                    .control(input)
                    .primary_action(primary_action)
                    .secondary_action(secondary_action)
                    .apply(Element::from)
                    .map(crate::pages::Message::PrinterDetails)
            }
        })
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::GoBack => self.go_back_task(),
            Message::LoadPrinter {
                printer,
                is_default,
                parent_page,
                queue_page,
                available_printers,
            } => {
                let printer_id = printer.id().to_string();
                self.load_printer(
                    printer,
                    is_default,
                    parent_page,
                    queue_page,
                    available_printers,
                );
                load_supplies_task(printer_id)
            }
            Message::SuppliesLoaded { printer_id, result } => {
                self.apply_supplies(printer_id, result);
                Task::none()
            }
            Message::CancelDialog => {
                self.dialog = None;
                Task::none()
            }
            Message::EditLocationChanged(location) => {
                self.update_location_draft(location);
                Task::none()
            }
            Message::SubmitLocation(printer_id, location) => {
                self.submit_location(printer_id, location)
            }
            Message::TogglePaperSizeDropdown(open) => {
                self.set_paper_size_dropdown_open(open);
                Task::none()
            }
            Message::TogglePrintSidesDropdown(open) => {
                self.set_print_sides_dropdown_open(open);
                Task::none()
            }
            Message::ToggleDefaultPrinter(printer_id, true) => {
                self.is_default = true;
                Self::set_default_printer_task(printer_id)
            }
            Message::ToggleDefaultPrinter(_, false) => Task::none(),
            Message::RemovePrinter(printer_id) => Self::delete_printer_task(printer_id),
            Message::PrinterDeleted(result) => self.finish_printer_deletion(result),
            Message::PrinterDefaultSet(result) => Self::finish_default_printer_update(result),
            Message::PrinterLocationSet(result) => Self::finish_location_update(result),
            Message::EditLocation(printer_id) => {
                self.open_location_dialog(printer_id);
                Task::none()
            }
            Message::SelectPaperSize(printer_id, index) => {
                self.select_paper_size(printer_id, index)
            }
            Message::SelectPrintSides(printer_id, index) => {
                self.select_print_sides(printer_id, index)
            }
            Message::PrinterOptionDefaultSet(result) => Self::finish_option_default_update(result),
            Message::OpenPrinterQueue(printer_id) => self.open_printer_queue(&printer_id),
        }
    }

    fn go_back_task(&self) -> Task<crate::Message> {
        cosmic::task::message(crate::app::Message::PageMessage(
            crate::pages::Message::Page(self.parent_page),
        ))
    }

    fn load_printer(
        &mut self,
        printer: PrinterEntry,
        is_default: bool,
        parent_page: page::Entity,
        queue_page: page::Entity,
        available_printers: Vec<PrinterEntry>,
    ) {
        self.printer = Some(printer);
        self.is_default = is_default;
        self.parent_page = parent_page;
        self.queue_page = queue_page;
        self.available_printers = available_printers;
        self.paper_size_dropdown_open = false;
        self.print_sides_dropdown_open = false;
        // What the last printer held says nothing about this one.
        self.supplies = Vec::new();
    }

    /// Keeps what a printer answered, if it is still the printer being shown.
    fn apply_supplies(&mut self, printer_id: String, result: Result<Vec<SupplyLevel>, String>) {
        if self.printer.as_ref().map(PrinterEntry::id) != Some(printer_id.as_str()) {
            return;
        }

        match result {
            Ok(supplies) => self.supplies = supplies,
            // A printer that cannot say what it holds shows no supplies, which is the
            // same as one that holds none it can report.
            Err(why) => {
                tracing::warn!(printer_id, why, "failed to load printer supplies");
                self.supplies = Vec::new();
            }
        }
    }

    fn update_location_draft(&mut self, location: String) {
        if let Some(Dialog::EditLocation {
            location: current, ..
        }) = &mut self.dialog
        {
            *current = location;
        }
    }

    fn submit_location(&mut self, printer_id: String, location: String) -> Task<crate::Message> {
        self.dialog = None;

        if let Some(printer) = self
            .printer
            .as_mut()
            .filter(|printer| printer.id() == printer_id)
        {
            printer.set_location(location.clone());
        }

        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::PrinterDetails(
                Message::PrinterLocationSet(
                    backend::set_printer_location(printer_id, location).await,
                ),
            ))
        })
    }

    fn set_paper_size_dropdown_open(&mut self, open: bool) {
        self.paper_size_dropdown_open = open;
        if open {
            self.print_sides_dropdown_open = false;
        }
    }

    fn set_print_sides_dropdown_open(&mut self, open: bool) {
        self.print_sides_dropdown_open = open;
        if open {
            self.paper_size_dropdown_open = false;
        }
    }

    fn set_default_printer_task(printer_id: String) -> Task<crate::Message> {
        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::PrinterDetails(
                Message::PrinterDefaultSet(backend::set_printer_default(printer_id).await),
            ))
        })
    }

    fn delete_printer_task(printer_id: String) -> Task<crate::Message> {
        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::PrinterDetails(
                Message::PrinterDeleted(backend::delete_printer(printer_id).await),
            ))
        })
    }

    fn finish_printer_deletion(&mut self, result: Result<(), String>) -> Task<crate::Message> {
        match result {
            Ok(()) => {
                self.printer = None;
                Task::batch([
                    Self::refresh_printers_task(),
                    cosmic::task::message(crate::app::Message::PageMessage(
                        crate::pages::Message::Page(self.parent_page),
                    )),
                ])
            }
            Err(why) => {
                tracing::warn!(why, "failed to delete printer");
                Task::none()
            }
        }
    }

    fn finish_default_printer_update(result: Result<(), String>) -> Task<crate::Message> {
        match result {
            Ok(()) => Self::refresh_printers_task(),
            Err(why) => {
                tracing::warn!(why, "failed to set default printer");
                Task::none()
            }
        }
    }

    fn finish_location_update(result: Result<(), String>) -> Task<crate::Message> {
        match result {
            Ok(()) => Self::refresh_printers_task(),
            Err(why) => {
                tracing::warn!(why, "failed to set printer location");
                Task::none()
            }
        }
    }

    fn open_location_dialog(&mut self, printer_id: String) {
        let location = self
            .printer
            .as_ref()
            .filter(|printer| printer.id() == printer_id)
            .and_then(|printer| printer.location().map(str::to_owned))
            .unwrap_or_default();

        self.dialog = Some(Dialog::EditLocation {
            printer_id,
            location,
        });
    }

    fn select_paper_size(&mut self, printer_id: String, index: usize) -> Task<crate::Message> {
        self.paper_size_dropdown_open = false;

        let value = self
            .printer
            .as_mut()
            .filter(|printer| printer.id() == printer_id)
            .and_then(|printer| {
                let value = printer.paper_sizes().get(index).cloned();
                if let Some(value) = &value {
                    printer.set_default_paper_size(value.clone());
                }
                value
            });

        let Some(value) = value else {
            return Task::none();
        };

        Self::set_option_default_task(printer_id, "media".into(), value)
    }

    fn select_print_sides(&mut self, printer_id: String, index: usize) -> Task<crate::Message> {
        self.print_sides_dropdown_open = false;

        let value = self
            .printer
            .as_mut()
            .filter(|printer| printer.id() == printer_id)
            .and_then(|printer| {
                let value = printer.print_sides().get(index).cloned();
                if let Some(value) = &value {
                    printer.set_default_print_sides(value.clone());
                }
                value
            });

        let Some(value) = value else {
            return Task::none();
        };

        Self::set_option_default_task(printer_id, "sides".into(), value)
    }

    fn set_option_default_task(
        printer_id: String,
        option: String,
        value: String,
    ) -> Task<crate::Message> {
        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::PrinterDetails(
                Message::PrinterOptionDefaultSet(
                    backend::set_printer_option_default(printer_id, option, value).await,
                ),
            ))
        })
    }

    fn finish_option_default_update(result: Result<(), String>) -> Task<crate::Message> {
        match result {
            Ok(()) => Self::refresh_printers_task(),
            Err(why) => {
                tracing::warn!(why, "failed to set printer option default");
                Task::none()
            }
        }
    }

    fn open_printer_queue(&self, printer_id: &str) -> Task<crate::Message> {
        let Some(printer) = self
            .printer
            .as_ref()
            .filter(|printer| printer.id() == printer_id)
        else {
            return Task::none();
        };

        Task::batch([
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::PrinterQueue(super::queue::Message::LoadPrinter {
                    printer: Box::new(printer.clone()),
                    available_printers: self.available_printers.clone(),
                }),
            )),
            cosmic::task::message(crate::app::Message::OpenContextDrawer(self.queue_page)),
        ])
    }

    fn refresh_printers_task() -> Task<crate::Message> {
        cosmic::task::message(crate::app::Message::PageMessage(
            crate::pages::Message::Printers(super::Message::Refresh),
        ))
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
) -> Element<'a, crate::pages::Message> {
    Element::from(
        widget::responsive(move |size| {
            container(
                details_content(page, printer, is_default)
                    .map(crate::pages::Message::PrinterDetails),
            )
            .width(Length::Fill)
            .align_x(Alignment::Start)
            .padding([
                0,
                super::adaptive_inner_padding(size.width),
                32,
                super::adaptive_inner_padding(size.width),
            ])
            .into()
        })
        .width(Length::Fill)
        .height(Length::Shrink),
    )
}

fn details_content<'a>(
    page: &'a Page,
    printer: &'a PrinterEntry,
    is_default: bool,
) -> Element<'a, Message> {
    column::with_capacity(5)
        .width(Length::Fill)
        .spacing(24)
        .push(default_queue_card(printer, is_default))
        .push(info_card(printer))
        .push(preferences_card(page, printer))
        .push_maybe(supplies_section(&page.supplies))
        .push(remove_printer_action(printer))
        .into()
}

fn details_header(printer: &PrinterEntry) -> Element<'static, Message> {
    container(
        column::with_capacity(2)
            .width(Length::Fill)
            .height(Length::Fixed(96.0))
            .push(back_button())
            .push(
                column::with_capacity(2)
                    .width(Length::Fill)
                    .height(Length::Fixed(64.0))
                    .padding([0, 0, 0, 16])
                    .align_x(Alignment::Start)
                    .push(
                        container(
                            text::body(printer.name().to_string())
                                .size(29)
                                .font(FONT_SEMIBOLD)
                                .class(TITLE_TEXT)
                                .wrapping(Wrapping::None)
                                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
                        )
                        .width(Length::Fill)
                        .height(Length::Fixed(43.0))
                        .align_y(Alignment::Center),
                    )
                    .push(status_line(&printer.status())),
            ),
    )
    .width(Length::Fill)
    .height(Length::Fixed(96.0))
    .padding([0, 0, 0, 16])
    .align_x(Alignment::Start)
    .into()
}

fn back_button() -> Element<'static, Message> {
    button::custom(
        row::with_capacity(2)
            .height(Length::Fixed(32.0))
            .align_y(Alignment::Center)
            .spacing(4)
            .push(widgets::symbolic_icon("go-previous-symbolic", 16, ACCENT))
            .push(text::body(fl!("printers")).size(14).class(ACCENT)),
    )
    .padding([0, 16])
    .width(Length::Fixed(104.0))
    .height(Length::Fixed(32.0))
    .class(cosmic::theme::Button::Transparent)
    .on_press(Message::GoBack)
    .into()
}

fn status_line(status: &PrinterStatus) -> Element<'static, Message> {
    let label = match status {
        PrinterStatus::Ready => fl!("printer-ready"),
        PrinterStatus::Offline => fl!("printer-offline"),
        PrinterStatus::LowToner => fl!("printer-low-toner"),
    };

    container(
        row::with_capacity(2)
            .width(Length::Fill)
            .height(Length::Fixed(21.0))
            .align_y(Alignment::Center)
            .spacing(8)
            .push(widgets::dot(status_color(status), 8.0))
            .push(text::body(label).size(14).class(TITLE_TEXT)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(21.0))
    .align_x(Alignment::Start)
    .align_y(Alignment::Center)
    .into()
}

fn status_color(status: &PrinterStatus) -> Color {
    match status {
        PrinterStatus::Ready => STATUS_READY,
        PrinterStatus::Offline | PrinterStatus::LowToner => STATUS_STOPPED,
    }
}

fn default_queue_card(printer: &PrinterEntry, is_default: bool) -> Element<'static, Message> {
    widgets::card(
        column::with_capacity(3)
            .push(settings_row(
                fl!("set-as-default-printer"),
                widget::toggler(is_default).on_toggle({
                    let id = printer.id().to_string();
                    move |value| Message::ToggleDefaultPrinter(id.clone(), value)
                }),
            ))
            .push(widgets::divider())
            .push(action_row(
                fl!("printer-queue"),
                printer.queue_status().unwrap_or_default().to_string(),
                Message::OpenPrinterQueue(printer.id().to_string()),
            )),
        97.0,
    )
}

fn info_card(printer: &PrinterEntry) -> Element<'static, Message> {
    widgets::card(
        column::with_capacity(7)
            .push(settings_row(
                fl!("location"),
                editable_value(
                    printer.location().unwrap_or_default().to_string(),
                    printer.id().to_string(),
                ),
            ))
            .push(widgets::divider())
            .push(value_row(
                fl!("model"),
                printer.model().unwrap_or_default().to_string(),
            ))
            .push(widgets::divider())
            .push(value_row(fl!("device-name"), printer.name().to_string()))
            .push(widgets::divider())
            .push(value_row(
                fl!("driver-version"),
                printer.driver_version().unwrap_or_default().to_string(),
            )),
        195.0,
    )
}

fn preferences_card(page: &Page, printer: &PrinterEntry) -> Element<'static, Message> {
    let paper_sizes = printer.paper_sizes();
    let print_sides = printer.print_sides();
    let paper_size_labels = paper_sizes
        .iter()
        .map(|value| media_label(value))
        .collect::<Vec<_>>();
    let print_sides_labels = print_sides
        .iter()
        .map(|value| sides_label(value))
        .collect::<Vec<_>>();
    let paper_size_idx = selected_option_idx(&paper_sizes, printer.default_paper_size(), 0);
    let print_sides_idx = selected_option_idx(&print_sides, printer.default_print_sides(), 0);

    widgets::card(
        column::with_capacity(3)
            .push(settings_row(
                fl!("paper-size"),
                widgets::dropdown_action(
                    selected_label(&paper_size_labels, paper_size_idx),
                    paper_size_labels,
                    Some(paper_size_idx),
                    page.paper_size_dropdown_open,
                    Message::TogglePaperSizeDropdown,
                    {
                        let id = printer.id().to_string();
                        move |idx| Message::SelectPaperSize(id.clone(), idx)
                    },
                    widgets::DropdownWidths {
                        trigger: 320.0,
                        popup: 320.0,
                    },
                ),
            ))
            .push(widgets::divider())
            .push(settings_row(
                fl!("print-sides"),
                widgets::dropdown_action(
                    selected_label(&print_sides_labels, print_sides_idx),
                    print_sides_labels,
                    Some(print_sides_idx),
                    page.print_sides_dropdown_open,
                    Message::TogglePrintSidesDropdown,
                    {
                        let id = printer.id().to_string();
                        move |idx| Message::SelectPrintSides(id.clone(), idx)
                    },
                    widgets::DropdownWidths {
                        trigger: 320.0,
                        popup: 320.0,
                    },
                ),
            )),
        97.0,
    )
}

fn selected_option_idx(values: &[String], default: Option<&str>, fallback: usize) -> usize {
    default
        .and_then(|default| values.iter().position(|value| value == default))
        .unwrap_or(fallback)
        .min(values.len().saturating_sub(1))
}

fn media_label(value: &str) -> String {
    let Some((name, size)) = media_name_and_size(value) else {
        return value.to_string();
    };

    format!("{name} ({size})")
}

fn media_name_and_size(value: &str) -> Option<(String, String)> {
    let (size_raw, unit) = value
        .strip_suffix("mm")
        .map(|size| (size, "mm"))
        .or_else(|| value.strip_suffix("in").map(|size| (size, "in")))?;
    let size_start = size_raw.rfind('_')? + 1;
    let dimensions = &size_raw[size_start..];
    let name_end = size_start.saturating_sub(1);
    let name_raw = value
        .get(..name_end)?
        .rsplit_once('_')
        .map(|(_, name)| name)
        .unwrap_or(value.get(..name_end)?);

    Some((
        pretty_media_name(name_raw),
        format!(
            "{} {}",
            dimensions.replace('x', " x "),
            if unit == "in" { "inches" } else { unit }
        ),
    ))
}

fn pretty_media_name(name: &str) -> String {
    match name {
        "a0" | "a1" | "a2" | "a3" | "a4" | "a5" | "a6" => name.to_uppercase(),
        "b0" | "b1" | "b2" | "b3" | "b4" | "b5" => name.to_uppercase(),
        "c0" | "c1" | "c2" | "c3" | "c4" | "c5" => name.to_uppercase(),
        "dl" => "DL".into(),
        other => other
            .split(['-', '_'])
            .filter(|part| !part.is_empty())
            .map(|part| {
                let mut chars = part.chars();
                match chars.next() {
                    Some(first) if part.chars().any(char::is_alphabetic) => {
                        format!("{}{}", first.to_uppercase(), chars.as_str())
                    }
                    Some(_) => part.to_string(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn sides_label(value: &str) -> String {
    match value {
        "one-sided" => fl!("print-one-side"),
        "two-sided-long-edge" => fl!("print-both-sides"),
        "two-sided-short-edge" => fl!("print-both-sides"),
        _ => value.to_string(),
    }
}

/// Asks the service what a printer holds.
fn load_supplies_task(printer_id: String) -> Task<crate::Message> {
    cosmic::task::future(async move {
        let result = backend::printer_supplies(printer_id.clone()).await;
        crate::Message::PageMessage(crate::pages::Message::PrinterDetails(
            Message::SuppliesLoaded { printer_id, result },
        ))
    })
}

/// How many supplies stand side by side.
const SUPPLY_COLUMNS: usize = 2;

/// Draws the supplies a printer reports, two to a row.
///
/// Rows are decided before layout runs, so which supplies share a row never changes
/// with the width of the pane — only how wide each of them is. A printer that reports
/// nothing gets no card at all rather than invented bars.
fn supplies_section(supplies: &[SupplyLevel]) -> Option<Element<'static, Message>> {
    if supplies.is_empty() {
        return None;
    }

    let rows = supply_rows(supplies.len());
    let mut grid = column::with_capacity(rows)
        .width(Length::Fill)
        .spacing(SUPPLY_ROW_SPACING);

    for chunk in supplies.chunks(SUPPLY_COLUMNS) {
        let mut cells = row::with_capacity(SUPPLY_COLUMNS)
            .width(Length::Fill)
            .height(Length::Fixed(SUPPLY_GRAPH_HEIGHT))
            .spacing(SUPPLY_COLUMN_SPACING);

        for supply in chunk {
            cells = cells.push(supply_graph(supply));
        }
        // A row holding one supply keeps the column the other would have used, so the
        // one above it is not stretched across the whole card.
        for _ in chunk.len()..SUPPLY_COLUMNS {
            cells = cells.push(horizontal_space());
        }

        grid = grid.push(cells);
    }

    Some(
        column::with_capacity(2)
            .width(Length::Fill)
            .spacing(8)
            .push(
                text::body(fl!("supplies"))
                    .size(14)
                    .font(FONT_SEMIBOLD)
                    .class(BODY_TEXT)
                    .height(Length::Fixed(21.0)),
            )
            .push(
                container(grid)
                    .width(Length::Fill)
                    .height(Length::Fixed(supplies_card_height(rows)))
                    .padding([SUPPLY_CARD_PADDING_Y as u16, 24])
                    .class(widgets::fill_container(CARD_BG, RADIUS_CARD)),
            )
            .into(),
    )
}

/// How many rows a count of supplies fills, two to a row.
fn supply_rows(supplies: usize) -> usize {
    supplies.div_ceil(SUPPLY_COLUMNS)
}

/// The height a card of supplies needs: its padding, a graph per row, and a gap
/// between rows.
fn supplies_card_height(rows: usize) -> f32 {
    let rows = rows.max(1) as f32;

    2.0 * SUPPLY_CARD_PADDING_Y
        + rows * SUPPLY_GRAPH_HEIGHT
        + (rows - 1.0) * f32::from(SUPPLY_ROW_SPACING)
}

fn remove_printer_action(printer: &PrinterEntry) -> Element<'static, Message> {
    container(
        row::with_capacity(2)
            .width(Length::Fill)
            .height(Length::Fixed(32.0))
            .align_y(Alignment::Center)
            .push(horizontal_space())
            .push(
                button::custom(
                    container(
                        text::body(fl!("remove-printer"))
                            .size(14)
                            .class(REMOVE_TEXT)
                            .align_x(Alignment::Center)
                            .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::Center)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fixed(134.0))
                .height(Length::Fixed(32.0))
                .padding(0)
                .class(widgets::pill_button_style(REMOVE_BG, REMOVE_TEXT))
                .on_press(Message::RemovePrinter(printer.id().to_string())),
            ),
    )
    .width(Length::Fill)
    .height(Length::Fixed(32.0))
    .into()
}

fn settings_row<'a>(
    label: String,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row::with_capacity(3)
        .width(Length::Fill)
        .height(Length::Fixed(ROW_HEIGHT))
        .padding([8, 24])
        .align_y(Alignment::Center)
        .spacing(16)
        .push(text::body(label).size(14).class(BODY_TEXT))
        .push(horizontal_space())
        .push(control)
        .into()
}

fn value_row(label: String, value: String) -> Element<'static, Message> {
    settings_row(label, right_value(value, 320.0))
}

fn editable_value(value: String, printer_id: String) -> Element<'static, Message> {
    container(
        row::with_capacity(2)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .spacing(8)
            .push(right_value(value, 312.0))
            .push(
                button::custom(widgets::symbolic_icon("edit-symbolic", 16, BODY_TEXT))
                    .width(Length::Fixed(32.0))
                    .height(Length::Fixed(32.0))
                    .padding(8)
                    .class(cosmic::theme::Button::Transparent)
                    .on_press(Message::EditLocation(printer_id)),
            ),
    )
    .width(Length::Fill)
    .max_width(352.0)
    .align_x(Alignment::End)
    .into()
}

fn action_row(label: String, value: String, message: Message) -> Element<'static, Message> {
    button::custom(
        row::with_capacity(4)
            .width(Length::Fill)
            .height(Length::Fixed(ROW_HEIGHT))
            .padding([8, 24])
            .align_y(Alignment::Center)
            .spacing(8)
            .push(text::body(label).size(14).class(BODY_TEXT))
            .push(horizontal_space())
            .push(right_value(value, 260.0))
            .push(widgets::symbolic_icon("go-next-symbolic", 16, BODY_TEXT)),
    )
    .padding(0)
    .width(Length::Fill)
    .height(Length::Fixed(ROW_HEIGHT))
    .class(cosmic::theme::Button::Transparent)
    .on_press(message)
    .into()
}

fn value_text(value: String) -> Element<'static, Message> {
    text::body(value)
        .size(14)
        .class(SECONDARY_TEXT)
        .align_x(Alignment::End)
        .wrapping(Wrapping::None)
        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
        .into()
}

fn right_value(value: String, width: f32) -> Element<'static, Message> {
    container(value_text(value))
        .width(Length::Fill)
        .max_width(width)
        .align_x(Alignment::End)
        .into()
}

/// One supply: what it is called, and how full it is.
fn supply_graph(supply: &SupplyLevel) -> Element<'static, Message> {
    let colors = bar_colors(supply);

    column::with_capacity(2)
        .width(Length::Fill)
        .height(Length::Fixed(SUPPLY_GRAPH_HEIGHT))
        // No gap: the label's line box and the bar's row stack straight onto each other.
        .push(
            row::with_capacity(2)
                .height(Length::Fixed(SUPPLY_LABEL_HEIGHT))
                .align_y(Alignment::Center)
                .spacing(8)
                .push(
                    text::body(supply_name(supply))
                        .size(14)
                        .class(TITLE_TEXT)
                        .width(Length::Fill)
                        .wrapping(Wrapping::None)
                        .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
                )
                // A cartridge holding several inks says which, where a bar so short it
                // is a few pixels wide could not.
                .push_maybe((colors.len() > 1).then(|| color_dots(&colors))),
        )
        .push(
            row::with_capacity(2)
                .height(Length::Fixed(SUPPLY_BAR_HEIGHT))
                .align_y(Alignment::Center)
                .spacing(0)
                .push(progress_track(supply, &colors))
                .push(supply_percentage(supply.level_percent)),
        )
        .into()
}

/// What to call a supply the printer named nothing.
fn supply_name(supply: &SupplyLevel) -> String {
    if supply.name.is_empty() {
        fl!("supply-unnamed")
    } else {
        supply.name.clone()
    }
}

/// The level as text.
///
/// A full supply is written without a decimal, which is the one value that would not
/// fit the width the design gives it.
fn supply_percentage(level: Option<u8>) -> Element<'static, Message> {
    container(
        text::body(percentage_label(level))
            .size(14)
            .class(TITLE_TEXT)
            .wrapping(Wrapping::None)
            .align_x(Alignment::Start),
    )
    .width(Length::Fixed(SUPPLY_PERCENTAGE_WIDTH))
    .height(Length::Fixed(SUPPLY_BAR_HEIGHT))
    .padding([0, 0, 0, 8])
    .align_y(Alignment::Center)
    .into()
}

fn percentage_label(level: Option<u8>) -> String {
    match level {
        Some(level) if level >= 100 => "100%".to_string(),
        Some(level) => format!("{:.1}%", f32::from(level)),
        None => fl!("supply-level-unknown"),
    }
}

/// The bar: how much is left, and where the printer says to take notice.
///
/// The mark sits on a layer of its own rather than inside the bar, so that the bar
/// stays one unbroken rounded shape and the level it shows is not eaten into.
fn progress_track(supply: &SupplyLevel, colors: &[Color]) -> Element<'static, Message> {
    let track = container(supply_fill(supply.level_percent, colors))
        .width(Length::Fill)
        .height(Length::Fixed(SUPPLY_TRACK_HEIGHT))
        .class(widgets::fill_container(SUPPLY_TRACK, RADIUS_SUPPLY_BAR));

    let Some(warning) = supply.warning else {
        return track.into();
    };

    cosmic::iced_widget::stack![
        container(track)
            .width(Length::Fill)
            .height(Length::Fixed(SUPPLY_BAR_HEIGHT))
            .align_y(Alignment::Center),
        warning_mark(warning, supply.level_percent),
    ]
    .width(Length::Fill)
    .height(Length::Fixed(SUPPLY_BAR_HEIGHT))
    .into()
}

/// How much of the track is filled, in the supply's own colours.
fn supply_fill(level: Option<u8>, colors: &[Color]) -> Element<'static, Message> {
    let mut bar = row::with_capacity(2).height(Length::Fixed(SUPPLY_TRACK_HEIGHT));
    // A level nothing reported fills nothing; the text beside it says as much.
    let filled = level.unwrap_or(0).min(100);
    let empty = 100_u8.saturating_sub(filled);

    if filled > 0 {
        bar = bar.push(
            container(band(supply_fill_color(colors)))
                .width(Length::FillPortion(u16::from(filled)))
                .height(Length::Fixed(SUPPLY_TRACK_HEIGHT)),
        );
    }

    // A portion of zero is laid out at full width and only then ignored, so an empty
    // side is left out rather than asked for.
    if empty > 0 {
        bar = bar.push(horizontal_space().width(Length::FillPortion(u16::from(empty))));
    }

    bar.into()
}

/// The one colour the filled part is drawn in.
///
/// A cartridge holding several inks has no single colour to be drawn in, so it takes the accent
/// and the dots beside the label carry its colours instead. A supply with one colour of its own
/// wears that colour, and one that reported none falls back to the neutral.
fn supply_fill_color(colors: &[Color]) -> Color {
    match colors {
        [] => SUPPLY_NEUTRAL,
        [only] => *only,
        _ => ACCENT,
    }
}

fn band(color: Color) -> container::Container<'static, Message, cosmic::Theme> {
    let radius = Radius::from(RADIUS_SUPPLY_BAR);
    let style = if needs_outline(color) {
        widgets::bordered_fill_container(color, DIVIDER, radius)
    } else {
        widgets::fill_container(color, radius)
    };

    container(horizontal_space())
        .width(Length::Fill)
        .height(Length::Fixed(SUPPLY_TRACK_HEIGHT))
        .class(style)
}

/// The mark showing where the printer says a supply needs attention.
///
/// It stands taller than the bar so that it can be seen over whatever colour is under
/// it, and turns to the warning colour once the level has reached it.
fn warning_mark(warning: SupplyWarning, level: Option<u8>) -> Element<'static, Message> {
    let reached = level.is_some_and(|level| warning.is_reached_by(level));
    let before = warning.level_percent.min(100);
    let after = 100_u8.saturating_sub(before);
    let mut marks = row::with_capacity(3)
        .width(Length::Fill)
        .height(Length::Fixed(SUPPLY_BAR_HEIGHT))
        .align_y(Alignment::Center);

    if before > 0 {
        marks = marks.push(horizontal_space().width(Length::FillPortion(u16::from(before))));
    }

    marks = marks.push(
        container(horizontal_space())
            .width(Length::Fixed(SUPPLY_MARK_WIDTH))
            .height(Length::Fixed(SUPPLY_MARK_HEIGHT))
            .class(widgets::fill_container(
                if reached { STATUS_STOPPED } else { TITLE_TEXT },
                1.0,
            )),
    );

    if after > 0 {
        marks = marks.push(horizontal_space().width(Length::FillPortion(u16::from(after))));
    }

    marks.into()
}

/// The colours to draw a supply in, each lifted until it can be seen on the card.
fn bar_colors(supply: &SupplyLevel) -> Vec<Color> {
    supply
        .colors
        .iter()
        .map(|color| visible_on_card(supply_color(*color)))
        .collect()
}

fn supply_color(color: SupplyRgb) -> Color {
    Color::from_rgba8(color.red, color.green, color.blue, 1.0)
}

/// Lifts a colour until its strongest channel clears what the card behind it needs,
/// leaving its hue alone.
///
/// Black has no hue to leave alone, so it becomes the neutral that floor names — which
/// is why a black cartridge draws as grey rather than as a bar that cannot be seen.
fn visible_on_card(color: Color) -> Color {
    let peak = color.r.max(color.g).max(color.b);
    if peak >= SUPPLY_MIN_CHANNEL {
        return color;
    }
    if peak <= f32::EPSILON {
        return SUPPLY_NEUTRAL;
    }

    let scale = SUPPLY_MIN_CHANNEL / peak;

    Color {
        r: (color.r * scale).min(1.0),
        g: (color.g * scale).min(1.0),
        b: (color.b * scale).min(1.0),
        ..color
    }
}

/// Returns whether a colour needs an edge drawn to be told from the track behind it.
///
/// Brightening cannot separate two greys, so the one case it does not answer is a
/// supply whose colour is close to the track's own.
fn needs_outline(color: Color) -> bool {
    let peak = color.r.max(color.g).max(color.b);
    let track = SUPPLY_TRACK.r.max(SUPPLY_TRACK.g).max(SUPPLY_TRACK.b);

    (peak - track).abs() < SUPPLY_OUTLINE_TOLERANCE
}

/// A dot per colour, for a cartridge that holds more than one.
fn color_dots(colors: &[Color]) -> Element<'static, Message> {
    let mut dots = row::with_capacity(colors.len())
        .height(Length::Fixed(SUPPLY_DOT_SIZE))
        .spacing(4);

    for color in colors {
        dots = dots.push(color_dot(*color));
    }

    dots.into()
}

fn color_dot(color: Color) -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fixed(SUPPLY_DOT_SIZE))
        .height(Length::Fixed(SUPPLY_DOT_SIZE))
        .class(widgets::bordered_fill_container(
            color,
            DIVIDER,
            RADIUS_PILL,
        ))
        .into()
}

fn selected_label(labels: &[String], selected: usize) -> String {
    labels.get(selected).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channels(color: Color) -> [u8; 3] {
        [
            (color.r * 255.0).round() as u8,
            (color.g * 255.0).round() as u8,
            (color.b * 255.0).round() as u8,
        ]
    }

    /// Black is a bar that cannot be seen on the card, so it is lifted to the grey the
    /// design draws it as. A colour bright enough already is left alone.
    #[test]
    fn a_supply_too_dark_to_see_is_lifted() {
        assert_eq!(channels(visible_on_card(Color::BLACK)), [0x9A, 0x9A, 0x9A]);

        for bright in [
            Color::from_rgba8(0x00, 0xFF, 0xFF, 1.0),
            Color::from_rgba8(0xFF, 0x00, 0xFF, 1.0),
            Color::from_rgba8(0xFF, 0xFF, 0x00, 1.0),
        ] {
            assert_eq!(channels(visible_on_card(bright)), channels(bright));
        }
    }

    /// Lifting a colour keeps its hue: a dark blue becomes a brighter blue, not grey.
    #[test]
    fn lifting_a_colour_keeps_its_hue() {
        let lifted = visible_on_card(Color::from_rgba8(0x00, 0x00, 0x80, 1.0));

        assert_eq!(channels(lifted), [0x00, 0x00, 0x9A]);
    }

    /// Brightening cannot separate two greys, so a supply the colour of the track gets
    /// an edge instead.
    #[test]
    fn a_supply_the_colour_of_the_track_is_outlined() {
        assert!(needs_outline(SUPPLY_TRACK));
        assert!(!needs_outline(SUPPLY_NEUTRAL));
        assert!(!needs_outline(Color::from_rgba8(0x00, 0xFF, 0xFF, 1.0)));
    }

    #[test]
    fn a_card_is_as_tall_as_the_rows_it_holds() {
        assert_eq!(supplies_card_height(1), 57.0);
        assert_eq!(supplies_card_height(2), 110.0);
        assert_eq!(supplies_card_height(3), 163.0);
        // No supplies draws no card, but the height must not go negative if asked.
        assert_eq!(supplies_card_height(0), 57.0);
    }

    #[test]
    fn supplies_fill_rows_two_at_a_time() {
        assert_eq!(
            (1..=5).map(supply_rows).collect::<Vec<_>>(),
            [1, 1, 2, 2, 3]
        );
    }

    /// Which supplies share a row is decided before layout, so it cannot change with
    /// the width of the pane, and the order is the one the printer reported.
    #[test]
    fn rows_keep_the_order_the_printer_reported() {
        let supplies = [0, 1, 2, 3, 4];
        let rows = supplies
            .chunks(SUPPLY_COLUMNS)
            .map(<[i32]>::to_vec)
            .collect::<Vec<_>>();

        assert_eq!(rows, [vec![0, 1], vec![2, 3], vec![4]]);
        assert_eq!(rows.concat(), supplies);
    }

    /// A cartridge holding several inks has no one colour to be drawn in, so the bar takes the
    /// accent and the dots say which colours it holds.
    #[test]
    fn a_supply_of_several_colours_is_drawn_in_the_accent() {
        let cyan = Color::from_rgba8(0x00, 0xFF, 0xFF, 1.0);
        let magenta = Color::from_rgba8(0xFF, 0x00, 0xFF, 1.0);
        let yellow = Color::from_rgba8(0xFF, 0xFF, 0x00, 1.0);

        assert_eq!(
            channels(supply_fill_color(&[cyan, magenta, yellow])),
            channels(ACCENT)
        );
        assert_eq!(channels(supply_fill_color(&[cyan])), channels(cyan));
        assert_eq!(channels(supply_fill_color(&[])), channels(SUPPLY_NEUTRAL));
    }

    #[test]
    fn a_full_supply_is_written_without_a_decimal() {
        assert_eq!(percentage_label(Some(100)), "100%");
        assert_eq!(percentage_label(Some(92)), "92.0%");
        assert_eq!(percentage_label(Some(0)), "0.0%");
    }
}
