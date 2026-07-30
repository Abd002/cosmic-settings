use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use cosmic::app::Task;
use cosmic::app::context_drawer::ContextDrawer;
use cosmic::iced::keyboard::Modifiers;
use cosmic::iced::{Alignment, Color, Length, Point};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{self, button, column, container, row, scrollable, text};
use cosmic::{Apply, Element};
use cosmic_settings_page::{self as page, Section, section};
use cosmic_settings_printers_client::{self as printers_client, CosmicPrintersProxy};
use cosmic_settings_printers_core::{JobInfo, JobState, PrinterEntry};
use slotmap::SlotMap;

use super::style::{
    ACCENT, BODY_TEXT, BUTTON_CANCEL, QUEUE_ERROR, QUEUE_FOREGROUND, QUEUE_LIST_BG,
    QUEUE_SELECTION_BG, STATUS_READY, TITLE_TEXT,
};
use super::{backend, widgets};

const QUEUE_CONTENT_PADDING: [u16; 4] = [0, 32, 32, 32];
const QUEUE_ROW_PADDING: [u16; 2] = [12, 24];
const QUEUE_ROW_SPACING: u16 = 16;
const QUEUE_CONTROLS_WIDTH: f32 = 64.0;
const QUEUE_SURFACE_HEIGHT: f32 = 600.0;
const QUEUE_MENU_WIDTH: f32 = 360.0;
const QUEUE_MENU_ROW_HEIGHT: u16 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum JobAction {
    Pause,
    Resume,
    Cancel,
}

impl JobAction {
    fn is_available_for(self, state: &JobState) -> bool {
        match self {
            Self::Pause => matches!(state, JobState::Pending | JobState::Processing),
            Self::Resume => matches!(state, JobState::Held | JobState::Stopped),
            Self::Cancel => !matches!(
                state,
                JobState::Completed | JobState::Canceled | JobState::Aborted | JobState::Failed
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct JobId(i32);

impl JobId {
    const fn from_raw(raw: i32) -> Self {
        Self(raw)
    }

    const fn into_raw(self) -> i32 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobOperation {
    action: JobAction,
    job_ids: Vec<JobId>,
}

impl JobOperation {
    fn new(action: JobAction, job_ids: impl IntoIterator<Item = JobId>) -> Option<Self> {
        let job_ids = job_ids.into_iter().collect::<Vec<_>>();
        (!job_ids.is_empty()).then_some(Self { action, job_ids })
    }

    fn single(action: JobAction, job_id: JobId) -> Self {
        Self {
            action,
            job_ids: vec![job_id],
        }
    }

    fn is_available_for(&self, jobs: &[JobInfo]) -> bool {
        self.job_ids.iter().all(|job_id| {
            jobs.iter().any(|job| {
                JobId::from_raw(job.id) == *job_id && self.action.is_available_for(&job.state)
            })
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueMenu {
    SelectedJobs,
    Global,
}

#[derive(Clone, Debug)]
pub enum Message {
    LoadPrinter {
        printer: Box<PrinterEntry>,
        available_printers: Vec<PrinterEntry>,
    },
    JobsLoaded {
        printer_id: String,
        result: Result<Vec<JobInfo>, String>,
    },
    SelectJob(JobId),
    ClearSelection,
    CursorMoved(Point),
    ModifiersChanged(Modifiers),
    OpenJobMenu(JobId),
    OpenGlobalMenu,
    CloseMenu,
    RunJobAction(JobOperation),
    JobActionFinished {
        printer_id: String,
        result: Result<(), String>,
    },
    Refresh,
    ToggleCompleted,
    OpenPrinterWebPage(String),
    PrinterWebPageOpened(Result<(), String>),
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
    printer: Option<PrinterEntry>,
    available_printers: Vec<PrinterEntry>,
    jobs: Vec<JobInfo>,
    loading: bool,
    error: Option<String>,
    selected_jobs: HashSet<JobId>,
    selection_anchor: Option<JobId>,
    show_completed: bool,
    modifiers: Modifiers,
    operation_in_flight: bool,
    menu: Option<QueueMenu>,
    cursor_position: Point,
    menu_position: Point,
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
        _sections: &mut SlotMap<section::Entity, Section<crate::pages::Message>>,
    ) -> Option<page::Content> {
        None
    }

    fn context_drawer(&self) -> Option<ContextDrawer<'_, crate::pages::Message>> {
        self.printer.as_ref()?;
        Some(
            cosmic::app::context_drawer(
                queue_view(self).map(crate::pages::Message::PrinterQueue),
                crate::pages::Message::CloseContextDrawer,
            )
            .title(fl!("printer-queue")),
        )
    }

    fn on_context_drawer_close(&mut self) -> cosmic::Task<crate::pages::Message> {
        self.clear_selection();
        cosmic::Task::none()
    }
}

impl Page {
    pub fn update(&mut self, message: Message) -> Task<crate::Message> {
        match message {
            Message::LoadPrinter {
                printer,
                available_printers,
            } => self.load_printer(*printer, available_printers),
            Message::JobsLoaded { printer_id, result } => {
                self.apply_jobs_loaded(printer_id, result)
            }
            Message::SelectJob(job_id) => {
                self.select_job(job_id);
                Task::none()
            }
            Message::ClearSelection => {
                self.clear_selection();
                Task::none()
            }
            Message::CursorMoved(position) => {
                self.cursor_position = position;
                Task::none()
            }
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
            Message::OpenJobMenu(job_id) => {
                self.open_job_menu(job_id);
                Task::none()
            }
            Message::OpenGlobalMenu => {
                self.open_global_menu();
                Task::none()
            }
            Message::CloseMenu => {
                self.menu = None;
                Task::none()
            }
            Message::RunJobAction(operation) => self.start_job_action(operation),
            Message::JobActionFinished { printer_id, result } => {
                self.finish_job_action(printer_id, result)
            }
            Message::Refresh => {
                self.menu = None;
                self.load_jobs_task()
            }
            Message::ToggleCompleted => {
                self.show_completed = !self.show_completed;
                self.clear_selection();
                self.load_jobs_task()
            }
            Message::OpenPrinterWebPage(web_page) => {
                self.menu = None;
                self.open_printer_web_page(web_page)
            }
            Message::PrinterWebPageOpened(result) => {
                if let Err(why) = result {
                    tracing::warn!(why, "failed to open printer web page");
                }
                Task::none()
            }
        }
    }

    fn load_printer(
        &mut self,
        printer: PrinterEntry,
        available_printers: Vec<PrinterEntry>,
    ) -> Task<crate::Message> {
        self.printer = Some(printer);
        self.available_printers = available_printers;
        self.jobs.clear();
        self.clear_selection();
        self.error = None;
        self.show_completed = false;
        self.load_jobs_task()
    }

    fn apply_jobs_loaded(
        &mut self,
        printer_id: String,
        result: Result<Vec<JobInfo>, String>,
    ) -> Task<crate::Message> {
        if !self.is_current_printer(&printer_id) {
            return Task::none();
        }

        self.loading = false;
        self.operation_in_flight = false;

        match result {
            Ok(jobs) => {
                self.selected_jobs
                    .retain(|job_id| jobs.iter().any(|job| JobId::from_raw(job.id) == *job_id));
                self.jobs = jobs;
                self.error = None;
            }
            Err(why) => {
                tracing::warn!(printer_id, why, "failed to load print jobs");
                self.error = Some(fl!("failed-to-load-print-jobs"));
            }
        }

        Task::none()
    }

    fn clear_selection(&mut self) {
        self.selected_jobs.clear();
        self.selection_anchor = None;
        self.menu = None;
    }

    fn open_job_menu(&mut self, job_id: JobId) {
        if !self.selected_jobs.contains(&job_id) {
            self.selected_jobs.clear();
            self.selected_jobs.insert(job_id);
            self.selection_anchor = Some(job_id);
        }
        self.menu_position = self.cursor_position;
        self.menu = Some(QueueMenu::SelectedJobs);
    }

    fn open_global_menu(&mut self) {
        self.selected_jobs.clear();
        self.selection_anchor = None;
        self.menu_position = self.cursor_position;
        self.menu = Some(QueueMenu::Global);
    }

    fn start_job_action(&mut self, operation: JobOperation) -> Task<crate::Message> {
        if self.operation_in_flight || !operation.is_available_for(&self.jobs) {
            return Task::none();
        }

        self.menu = None;

        let Some(printer_id) = self.printer.as_ref().map(|printer| printer.id.clone()) else {
            return Task::none();
        };

        self.operation_in_flight = true;

        cosmic::task::future(async move {
            let result = run_job_operation(printer_id.clone(), operation).await;
            crate::Message::PageMessage(crate::pages::Message::PrinterQueue(
                Message::JobActionFinished { printer_id, result },
            ))
        })
    }

    fn finish_job_action(
        &mut self,
        printer_id: String,
        result: Result<(), String>,
    ) -> Task<crate::Message> {
        if !self.is_current_printer(&printer_id) {
            return Task::none();
        }

        self.operation_in_flight = false;
        if let Err(why) = result {
            tracing::warn!(printer_id, why, "print job operation failed");
            self.error = Some(why);
        }

        Task::batch([
            self.load_jobs_task(),
            cosmic::task::message(crate::app::Message::PageMessage(
                crate::pages::Message::Printers(super::Message::Refresh),
            )),
        ])
    }

    fn open_printer_web_page(&mut self, web_page: String) -> Task<crate::Message> {
        cosmic::task::future(async move {
            crate::Message::PageMessage(crate::pages::Message::PrinterQueue(
                Message::PrinterWebPageOpened(backend::open_printer_web_page(web_page).await),
            ))
        })
    }

    fn is_current_printer(&self, printer_id: &str) -> bool {
        self.printer.as_ref().map(|printer| printer.id.as_str()) == Some(printer_id)
    }

    fn select_job(&mut self, job_id: JobId) {
        if self.modifiers.shift() {
            let Some(anchor) = self.selection_anchor else {
                self.selected_jobs.clear();
                self.selected_jobs.insert(job_id);
                self.selection_anchor = Some(job_id);
                return;
            };
            let Some(anchor_idx) = self
                .jobs
                .iter()
                .position(|job| JobId::from_raw(job.id) == anchor)
            else {
                return;
            };
            let Some(job_idx) = self
                .jobs
                .iter()
                .position(|job| JobId::from_raw(job.id) == job_id)
            else {
                return;
            };
            let (start, end) = if anchor_idx <= job_idx {
                (anchor_idx, job_idx)
            } else {
                (job_idx, anchor_idx)
            };
            if !self.modifiers.control() {
                self.selected_jobs.clear();
            }
            self.selected_jobs.extend(
                self.jobs[start..=end]
                    .iter()
                    .map(|job| JobId::from_raw(job.id)),
            );
        } else if self.modifiers.control() {
            if !self.selected_jobs.remove(&job_id) {
                self.selected_jobs.insert(job_id);
            }
            self.selection_anchor = Some(job_id);
        } else {
            self.selected_jobs.clear();
            self.selected_jobs.insert(job_id);
            self.selection_anchor = Some(job_id);
        }
    }

    fn load_jobs_task(&mut self) -> Task<crate::Message> {
        let Some(printer) = &self.printer else {
            return Task::none();
        };
        self.loading = true;
        let printer_id = printer.id.clone();
        let include_completed = self.show_completed;
        cosmic::task::future(async move {
            let result = load_jobs(printer_id.clone(), include_completed).await;
            crate::Message::PageMessage(crate::pages::Message::PrinterQueue(Message::JobsLoaded {
                printer_id,
                result,
            }))
        })
    }
}

fn queue_view(page: &Page) -> Element<'_, Message> {
    let body: Element<'_, Message> = if page.loading {
        queue_message(fl!("loading-print-jobs"))
    } else if let Some(error) = &page.error {
        queue_message(error.clone())
    } else if page.jobs.is_empty() {
        queue_message(fl!("no-jobs-waiting"))
    } else {
        queue_jobs(page)
    };

    let cancelable = job_ids_for_action(&page.jobs, JobAction::Cancel);
    let mut content = column::with_capacity(2)
        .push(body)
        .spacing(24)
        .width(Length::Fill)
        .height(Length::Fill);

    if !cancelable.is_empty() {
        let cancel_all =
            JobOperation::new(JobAction::Cancel, cancelable).map(Message::RunJobAction);
        content = content.push(
            button::custom(
                container(text::body(fl!("cancel-all")).class(TITLE_TEXT))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .center(Length::Fill),
            )
            .padding([0, 16])
            .width(Length::Fixed(94.0))
            .height(Length::Fixed(32.0))
            .class(widgets::pill_button_style(BUTTON_CANCEL, TITLE_TEXT))
            .on_press_maybe((!page.operation_in_flight).then_some(cancel_all).flatten())
            .apply(container)
            .width(Length::Fill)
            .align_x(Alignment::End),
        );
    }

    let queue_surface = widget::mouse_area(
        container(content)
            .padding(QUEUE_CONTENT_PADDING)
            .width(Length::Fill)
            .height(Length::Fixed(QUEUE_SURFACE_HEIGHT)),
    )
    .on_move(Message::CursorMoved)
    .on_press(Message::ClearSelection)
    .on_right_press(Message::OpenGlobalMenu);

    let popup = match page.menu {
        Some(QueueMenu::SelectedJobs) => Some(selected_jobs_menu(page)),
        Some(QueueMenu::Global) => Some(global_queue_menu(page)),
        None => None,
    };

    if let Some(popup) = popup {
        widget::popover(queue_surface)
            .position(widget::popover::Position::Point(page.menu_position))
            .popup(popup)
            .on_close(Message::CloseMenu)
            .into()
    } else {
        queue_surface.into()
    }
}

fn queue_jobs(page: &Page) -> Element<'_, Message> {
    let mut rows = column::with_capacity(page.jobs.len().saturating_mul(2));
    for (index, job) in page.jobs.iter().enumerate() {
        rows = rows.push(job_row(page, job));
        if index + 1 < page.jobs.len() {
            rows = rows.push(widgets::divider());
        }
    }

    scrollable(
        container(rows)
            .width(Length::Fill)
            .class(widgets::fill_container(QUEUE_LIST_BG, 8.0)),
    )
    .width(Length::Fill)
    .height(Length::Shrink)
    .into()
}

fn job_row(page: &Page, job: &JobInfo) -> Element<'static, Message> {
    let job_id = JobId::from_raw(job.id);
    let selected = page.selected_jobs.contains(&job_id);
    let content = row::with_capacity(2)
        .push(job_copy(job, selected))
        .push(job_controls(page, job, selected))
        .spacing(QUEUE_ROW_SPACING)
        .align_y(Alignment::Center)
        .padding(QUEUE_ROW_PADDING)
        .width(Length::Fill);

    let row = container(content).width(Length::Fill);
    let row = if selected {
        row.class(widgets::fill_container(QUEUE_SELECTION_BG, 0.0))
    } else {
        row
    };

    let trigger = widget::mouse_area(row)
        .on_press(Message::SelectJob(job_id))
        .on_right_press(Message::OpenJobMenu(job_id));

    cosmic::iced_widget::opaque(trigger)
}

fn job_copy(job: &JobInfo, selected: bool) -> Element<'static, Message> {
    let foreground = queue_row_foreground(selected);
    let title = text::body(if job.title.trim().is_empty() {
        fl!("untitled-print-job")
    } else {
        job.title.clone()
    })
    .size(14)
    .class(cosmic::theme::Text::Color(foreground))
    .wrapping(Wrapping::None)
    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
    .width(Length::Fill);

    column::with_capacity(2)
        .push(title)
        .push(job_metadata(job, selected))
        .spacing(0)
        .width(Length::Fill)
        .into()
}

fn job_controls(page: &Page, job: &JobInfo, selected: bool) -> Element<'static, Message> {
    let foreground = queue_row_foreground(selected);
    let mut controls = row::with_capacity(2)
        .width(Length::Fixed(QUEUE_CONTROLS_WIDTH))
        .height(Length::Fixed(32.0));

    let primary = match job.state {
        JobState::Processing => Some(("media-playback-pause-symbolic", JobAction::Pause)),
        JobState::Held | JobState::Stopped => {
            Some(("media-playback-start-symbolic", JobAction::Resume))
        }
        _ => None,
    };
    if let Some((icon, action)) = primary {
        let operation = JobOperation::single(action, JobId::from_raw(job.id));
        controls = controls.push(queue_icon_button(
            icon,
            foreground,
            (!page.operation_in_flight).then_some(Message::RunJobAction(operation)),
        ));
    } else if matches!(
        job.state,
        JobState::Pending | JobState::Aborted | JobState::Failed
    ) {
        // Refresh reloads server state. A true retry remains disabled until
        // cosmic-printers exposes Restart-Job.
        controls = controls.push(queue_icon_button(
            "view-refresh-symbolic",
            foreground,
            Some(Message::Refresh),
        ));
    } else {
        controls = controls.push(widget::space::horizontal().width(Length::Fixed(32.0)));
    }

    controls = controls.push(queue_icon_button(
        "window-close-symbolic",
        foreground,
        (JobAction::Cancel.is_available_for(&job.state) && !page.operation_in_flight).then_some(
            Message::RunJobAction(JobOperation::single(
                JobAction::Cancel,
                JobId::from_raw(job.id),
            )),
        ),
    ));

    controls.into()
}

fn queue_icon_button(
    name: &'static str,
    color: Color,
    message: Option<Message>,
) -> Element<'static, Message> {
    button::custom(widgets::symbolic_icon(name, 16, color))
        .padding(8)
        .width(Length::Fixed(32.0))
        .height(Length::Fixed(32.0))
        .class(cosmic::theme::Button::Transparent)
        .on_press_maybe(message)
        .into()
}

fn selected_jobs_menu(page: &Page) -> Element<'static, Message> {
    let selected = page
        .jobs
        .iter()
        .filter(|job| page.selected_jobs.contains(&JobId::from_raw(job.id)))
        .collect::<Vec<_>>();
    let ids = selected
        .iter()
        .map(|job| JobId::from_raw(job.id))
        .collect::<Vec<_>>();
    let all_pause = !selected.is_empty()
        && selected
            .iter()
            .all(|job| JobAction::Pause.is_available_for(&job.state));
    let all_resume = !selected.is_empty()
        && selected
            .iter()
            .all(|job| JobAction::Resume.is_available_for(&job.state));
    let all_cancel = !selected.is_empty()
        && selected
            .iter()
            .all(|job| JobAction::Cancel.is_available_for(&job.state));
    let web_page = page
        .printer
        .as_ref()
        .and_then(|printer| printer.web_page.clone());

    menu_surface(
        column::with_capacity(9)
            .push(menu_row(
                fl!("cancel"),
                all_cancel
                    .then(|| JobOperation::new(JobAction::Cancel, ids.clone()))
                    .flatten()
                    .map(Message::RunJobAction),
            ))
            .push(menu_row(
                fl!("pause"),
                all_pause
                    .then(|| JobOperation::new(JobAction::Pause, ids.clone()))
                    .flatten()
                    .map(Message::RunJobAction),
            ))
            .push(menu_row(
                fl!("resume"),
                all_resume
                    .then(|| JobOperation::new(JobAction::Resume, ids.clone()))
                    .flatten()
                    .map(Message::RunJobAction),
            ))
            .push(menu_row(fl!("refresh"), Some(Message::Refresh)))
            .push(widgets::inset_divider(8))
            .push(move_to_printer_row(page))
            .push(widgets::inset_divider(8))
            .push(menu_row(
                fl!("printer-web-interface"),
                web_page.map(Message::OpenPrinterWebPage),
            )),
    )
}

fn global_queue_menu(page: &Page) -> Element<'static, Message> {
    let cancelable = job_ids_for_action(&page.jobs, JobAction::Cancel);
    let pausable = job_ids_for_action(&page.jobs, JobAction::Pause);
    let resumable = job_ids_for_action(&page.jobs, JobAction::Resume);
    let web_page = page
        .printer
        .as_ref()
        .and_then(|printer| printer.web_page.clone());

    menu_surface(
        column::with_capacity(11)
            .push(menu_row(
                fl!("cancel-all"),
                JobOperation::new(JobAction::Cancel, cancelable).map(Message::RunJobAction),
            ))
            .push(menu_row(
                fl!("pause-all"),
                JobOperation::new(JobAction::Pause, pausable).map(Message::RunJobAction),
            ))
            .push(menu_row(
                fl!("resume-all"),
                JobOperation::new(JobAction::Resume, resumable).map(Message::RunJobAction),
            ))
            .push(menu_row(fl!("refresh-all"), Some(Message::Refresh)))
            .push(widgets::inset_divider(8))
            .push(menu_toggle_row(
                fl!("show-completed-jobs"),
                page.show_completed,
                Some(Message::ToggleCompleted),
            ))
            .push(widgets::inset_divider(8))
            .push(move_to_printer_row(page))
            .push(widgets::inset_divider(8))
            .push(menu_row(
                fl!("printer-web-interface"),
                web_page.map(Message::OpenPrinterWebPage),
            )),
    )
}

fn move_to_printer_row(_page: &Page) -> Element<'static, Message> {
    button::custom(
        row::with_capacity(2)
            .push(
                text::body(fl!("move-to-printer"))
                    .size(14)
                    .class(cosmic::theme::Text::Color(BODY_TEXT))
                    .width(Length::Fill),
            )
            .push(widgets::symbolic_icon("go-next-symbolic", 16, BODY_TEXT))
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding([4, 16])
    .height(Length::Fixed(f32::from(QUEUE_MENU_ROW_HEIGHT)))
    .width(Length::Fill)
    .class(cosmic::theme::Button::Transparent)
    // Do not pretend this action succeeded. Enable it only after the backend
    // exposes Move-Job and the queue has a real destination-selection flow.
    .on_press_maybe(None)
    .into()
}

fn menu_toggle_row(
    label: String,
    checked: bool,
    message: Option<Message>,
) -> Element<'static, Message> {
    let indicator: Element<'static, Message> = if checked {
        widgets::symbolic_icon("object-select-symbolic", 16, BODY_TEXT).into()
    } else {
        widget::space::horizontal()
            .width(Length::Fixed(16.0))
            .into()
    };

    button::custom(
        row::with_capacity(2)
            .push(indicator)
            .push(
                text::body(label)
                    .size(14)
                    .class(cosmic::theme::Text::Color(BODY_TEXT))
                    .width(Length::Fill),
            )
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding([4, 16])
    .height(Length::Fixed(f32::from(QUEUE_MENU_ROW_HEIGHT)))
    .width(Length::Fill)
    .class(cosmic::theme::Button::Transparent)
    .on_press_maybe(message)
    .into()
}

fn menu_row(label: String, message: Option<Message>) -> Element<'static, Message> {
    button::custom(
        text::body(label)
            .size(14)
            .class(cosmic::theme::Text::Color(BODY_TEXT))
            .width(Length::Fill),
    )
    .padding([4, 16])
    .height(Length::Fixed(f32::from(QUEUE_MENU_ROW_HEIGHT)))
    .width(Length::Fill)
    .class(cosmic::theme::Button::Transparent)
    .on_press_maybe(message)
    .into()
}

fn menu_surface(content: impl Into<Element<'static, Message>>) -> Element<'static, Message> {
    container(content)
        .padding([8, 0])
        .width(Length::Fixed(QUEUE_MENU_WIDTH))
        .class(widgets::context_menu_container())
        .into()
}

fn queue_message(label: String) -> Element<'static, Message> {
    container(text::body(label).size(14).class(TITLE_TEXT))
        .center_x(Length::Fill)
        .width(Length::Fill)
        .into()
}

fn queue_row_foreground(selected: bool) -> Color {
    if selected { ACCENT } else { QUEUE_FOREGROUND }
}

fn job_metadata(job: &JobInfo, selected: bool) -> Element<'static, Message> {
    let foreground = queue_row_foreground(selected);
    let mut values = Vec::with_capacity(4);

    if !job.user.trim().is_empty() {
        values.push((job.user.clone(), foreground));
    }
    if job.size > 0 {
        values.push((format_job_size_k_octets(job.size), foreground));
    }
    if job.creation_time > 0 {
        values.push((
            relative_time_from_unix_timestamp(job.creation_time),
            foreground,
        ));
    }
    values.push((
        job_state_label(&job.state),
        job_state_color(&job.state, selected),
    ));

    let mut children = Vec::with_capacity(values.len().saturating_mul(2));
    for (index, (label, color)) in values.into_iter().enumerate() {
        if index > 0 {
            children.push(widgets::dot(foreground, 2.0));
        }
        children.push(
            text::caption(label)
                .class(cosmic::theme::Text::Color(color))
                .into(),
        );
    }

    widget::flex_row(children)
        .spacing(4)
        .align_items(Alignment::Center)
        .width(Length::Fill)
        .into()
}

fn format_job_size_k_octets(k_octets: i32) -> String {
    if k_octets >= 1024 {
        format!("{:.1} MB", f64::from(k_octets) / 1024.0)
    } else {
        format!("{k_octets} KB")
    }
}

fn relative_time_from_unix_timestamp(timestamp: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(timestamp);

    relative_time_from(now, timestamp)
}

fn relative_time_from(now: i64, timestamp: i64) -> String {
    let elapsed = now.saturating_sub(timestamp);
    if elapsed < 60 {
        fl!("job-time-just-now")
    } else if elapsed < 3600 {
        let count = elapsed / 60_i64;
        fl!("job-time-minutes", count = count)
    } else if elapsed < 86_400 {
        let count = elapsed / 3_600_i64;
        fl!("job-time-hours", count = count)
    } else {
        let count = elapsed / 86_400_i64;
        fl!("job-time-days", count = count)
    }
}

fn job_state_label(state: &JobState) -> String {
    match state {
        JobState::Pending => fl!("job-pending"),
        JobState::Processing => fl!("job-printing"),
        JobState::Completed => fl!("job-completed"),
        JobState::Canceled => fl!("job-canceled"),
        JobState::Aborted | JobState::Failed => fl!("job-error"),
        JobState::Held => fl!("job-paused"),
        JobState::Stopped => fl!("job-stopped"),
        JobState::Unknown => fl!("job-unknown"),
    }
}

fn job_ids_for_action(jobs: &[JobInfo], action: JobAction) -> Vec<JobId> {
    jobs.iter()
        .filter(|job| action.is_available_for(&job.state))
        .map(|job| JobId::from_raw(job.id))
        .collect()
}

async fn load_jobs(printer_id: String, include_completed: bool) -> Result<Vec<JobInfo>, String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    let filter = if include_completed { "all" } else { "active" };
    let reply = client
        .conn
        .get_jobs(printer_id, filter.to_string())
        .await
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"))?;
    Ok(reply.jobs)
}

async fn run_job_operation(printer_id: String, operation: JobOperation) -> Result<(), String> {
    let mut client = printers_client::connect()
        .await
        .map_err(|why| why.to_string())?;
    // Run the requests sequentially so the first backend failure stops the batch,
    // matching the queue's existing operation semantics.
    for job_id in operation.job_ids {
        let job_id = job_id.into_raw();
        let result = match operation.action {
            JobAction::Pause => client.conn.pause_job(printer_id.clone(), job_id).await,
            JobAction::Resume => client.conn.resume_job(printer_id.clone(), job_id).await,
            JobAction::Cancel => client.conn.cancel_job(printer_id.clone(), job_id).await,
        }
        .map_err(|why| why.to_string())?
        .map_err(|why| format!("{why:?}"));
        result?;
    }
    Ok(())
}

fn job_state_color(state: &JobState, selected: bool) -> Color {
    match state {
        JobState::Processing => STATUS_READY,
        JobState::Aborted | JobState::Failed => QUEUE_ERROR,
        _ => queue_row_foreground(selected),
    }
}
