use cosmic::app::Task;
use cosmic::iced::{Alignment, Color, Length, Subscription, event, keyboard};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{
    self, button, column, container, row, space::horizontal as horizontal_space, text,
};
use cosmic::{Apply, Element};
use cosmic_settings_page::{self as page, Section, section};
use cosmic_settings_printers_core::{PrinterStatus, SupplyLevel};
use slotmap::SlotMap;

use super::style::{
    ACCENT, BLACK_SUPPLY, BODY_TEXT, CARD_BG, DIVIDER, FONT_SEMIBOLD, RADIUS_CARD, REMOVE_BG,
    REMOVE_TEXT, SECONDARY_TEXT, STATUS_READY, STATUS_STOPPED, SUPPLY_TRACK, TITLE_TEXT,
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
    dialog: Option<Dialog>,
    is_default: bool,
    paper_size_dropdown_open: bool,
    print_sides_dropdown_open: bool,
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
            dialog: None,
            is_default: false,
            paper_size_dropdown_open: false,
            print_sides_dropdown_open: false,
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
            } => {
                self.load_printer(printer, is_default, parent_page, queue_page);
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
    ) {
        self.printer = Some(printer);
        self.is_default = is_default;
        self.parent_page = parent_page;
        self.queue_page = queue_page;
        self.paper_size_dropdown_open = false;
        self.print_sides_dropdown_open = false;
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
            .filter(|printer| printer.id == printer_id)
        {
            printer.location = location.clone();
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
            .filter(|printer| printer.id == printer_id)
            .map(|printer| printer.location.clone())
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
            .filter(|printer| printer.id == printer_id)
            .and_then(|printer| {
                printer.paper_size_idx = index;
                let value = printer.paper_sizes.get(index).cloned();
                if let Some(value) = &value {
                    printer
                        .options
                        .insert("media-default".into(), value.clone());
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
            .filter(|printer| printer.id == printer_id)
            .and_then(|printer| {
                printer.print_sides_idx = index;
                let value = printer.print_sides.get(index).cloned();
                if let Some(value) = &value {
                    printer
                        .options
                        .insert("sides-default".into(), value.clone());
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
            .filter(|printer| printer.id.as_str() == printer_id)
        else {
            return Task::none();
        };

        Task::batch([
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::PrinterQueue(super::queue::Message::LoadPrinter {
                    printer: Box::new(printer.clone()),
                    available_printers: vec![printer.clone()],
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
        .push(supplies_section(printer))
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
                            text::body(printer.name.clone())
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
                    .push(status_line(&printer.status)),
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
                    let id = printer.id.clone();
                    move |value| Message::ToggleDefaultPrinter(id.clone(), value)
                }),
            ))
            .push(widgets::divider())
            .push(action_row(
                fl!("printer-queue"),
                printer.queue_status.clone(),
                Message::OpenPrinterQueue(printer.id.clone()),
            )),
        97.0,
    )
}

fn info_card(printer: &PrinterEntry) -> Element<'static, Message> {
    widgets::card(
        column::with_capacity(7)
            .push(settings_row(
                fl!("location"),
                editable_value(printer.location.clone(), printer.id.clone()),
            ))
            .push(widgets::divider())
            .push(value_row(fl!("model"), printer.model.clone()))
            .push(widgets::divider())
            .push(value_row(fl!("device-name"), printer.name.clone()))
            .push(widgets::divider())
            .push(value_row(
                fl!("driver-version"),
                printer.driver_version.clone(),
            )),
        195.0,
    )
}

fn preferences_card(page: &Page, printer: &PrinterEntry) -> Element<'static, Message> {
    let paper_size_labels = printer
        .paper_sizes
        .iter()
        .map(|value| media_label(value))
        .collect::<Vec<_>>();
    let print_sides_labels = printer
        .print_sides
        .iter()
        .map(|value| sides_label(value))
        .collect::<Vec<_>>();
    let paper_size_idx = selected_option_idx(
        &printer.paper_sizes,
        printer.options.get("media-default"),
        printer.paper_size_idx,
    );
    let print_sides_idx = selected_option_idx(
        &printer.print_sides,
        printer.options.get("sides-default"),
        printer.print_sides_idx,
    );

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
                        let id = printer.id.clone();
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
                        let id = printer.id.clone();
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

fn selected_option_idx(values: &[String], default: Option<&String>, fallback: usize) -> usize {
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

fn supplies_section(printer: &PrinterEntry) -> Element<'static, Message> {
    let color_supply = find_supply(&printer.supplies, "tri")
        .or_else(|| find_supply(&printer.supplies, "color"))
        .or_else(|| printer.supplies.first());
    let black_supply = find_supply(&printer.supplies, "black")
        .or_else(|| printer.supplies.get(1))
        .or(color_supply);

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
            container(
                row::with_capacity(2)
                    .width(Length::Fill)
                    .height(Length::Fixed(45.0))
                    .spacing(16)
                    .align_y(Alignment::Center)
                    .push(supply_graph(
                        supply_label(color_supply, "Tricolor cartridge"),
                        color_supply.map_or(0, |supply| supply.level_percent),
                        ACCENT,
                        true,
                    ))
                    .push(supply_graph(
                        supply_label(black_supply, "Black"),
                        black_supply.map_or(0, |supply| supply.level_percent),
                        BLACK_SUPPLY,
                        false,
                    )),
            )
            .width(Length::Fill)
            .height(Length::Fixed(61.0))
            .padding([8, 24])
            .class(widgets::fill_container(CARD_BG, RADIUS_CARD)),
        )
        .into()
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
                .on_press(Message::RemovePrinter(printer.id.clone())),
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

fn supply_graph(
    label: String,
    percent: u8,
    fill: Color,
    tricolor: bool,
) -> Element<'static, Message> {
    column::with_capacity(2)
        .width(Length::Fixed(272.0))
        .height(Length::Fixed(45.0))
        .spacing(4)
        .push(
            row::with_capacity(2)
                .height(Length::Fixed(21.0))
                .align_y(Alignment::Center)
                .spacing(8)
                .push(text::body(label).size(14).class(BODY_TEXT))
                .push_maybe(tricolor.then(tricolor_indicator)),
        )
        .push(
            row::with_capacity(2)
                .height(Length::Fixed(20.0))
                .align_y(Alignment::Center)
                .spacing(0)
                .push(progress_track(percent, fill))
                .push(
                    container(
                        text::body(format!("{:.1}%", percent as f32))
                            .size(14)
                            .class(TITLE_TEXT)
                            .align_x(Alignment::Start),
                    )
                    .width(Length::Fixed(48.0))
                    .height(Length::Fixed(20.0))
                    .padding([0, 0, 0, 8])
                    .align_y(Alignment::Center),
                ),
        )
        .into()
}

fn progress_track(percent: u8, fill: Color) -> Element<'static, Message> {
    let fill_portion = percent.min(100) as u16;
    let empty_portion = 100_u16.saturating_sub(fill_portion);
    let mut bar = row::with_capacity(2).height(Length::Fixed(12.0));

    if fill_portion > 0 {
        bar = bar.push(
            container(horizontal_space())
                .width(Length::FillPortion(fill_portion))
                .height(Length::Fixed(12.0))
                .class(widgets::fill_container(fill, 40.0)),
        );
    }

    if empty_portion > 0 {
        bar = bar.push(horizontal_space().width(Length::FillPortion(empty_portion)));
    }

    container(bar)
        .width(Length::Fixed(224.0))
        .height(Length::Fixed(12.0))
        .class(widgets::fill_container(SUPPLY_TRACK, 40.0))
        .into()
}

fn supply_label(supply: Option<&SupplyLevel>, fallback: &str) -> String {
    let label = supply
        .map(|supply| supply.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback.into());

    if label == "Tricolor" {
        "Tricolor cartridge".into()
    } else {
        label
    }
}

fn tricolor_indicator() -> Element<'static, Message> {
    row::with_capacity(3)
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(8.0))
        .spacing(4)
        .push(color_dot(Color::from_rgb(0.0, 1.0, 1.0)))
        .push(color_dot(Color::from_rgb(1.0, 0.0, 1.0)))
        .push(color_dot(Color::from_rgb(1.0, 1.0, 0.0)))
        .into()
}

fn color_dot(color: Color) -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fixed(8.0))
        .height(Length::Fixed(8.0))
        .class(widgets::bordered_fill_container(color, DIVIDER, 160.0))
        .into()
}

fn find_supply<'a>(supplies: &'a [SupplyLevel], needle: &str) -> Option<&'a SupplyLevel> {
    supplies
        .iter()
        .find(|supply| supply.name.to_lowercase().contains(needle))
}

fn selected_label(labels: &[String], selected: usize) -> String {
    labels.get(selected).cloned().unwrap_or_default()
}
