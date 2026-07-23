use std::sync::Arc;

use cosmic::Element;
use cosmic::iced::{Alignment, Background, Border, Color, Length, Shadow, Vector};
use cosmic::iced_core::text::{Ellipsize, EllipsizeHeightLimit, Wrapping};
use cosmic::widget::{
    self, button, column, container, icon, row, space::horizontal as horizontal_space, text,
};

use super::style;

#[derive(Clone, Copy, Debug)]
pub struct DropdownWidths {
    pub trigger: f32,
    pub popup: f32,
}

pub fn symbolic_icon(name: &'static str, size: u16, color: Color) -> icon::Icon {
    icon::from_name(name)
        .size(size)
        .icon()
        .class(cosmic::theme::Svg::custom(move |_| {
            cosmic::iced_widget::svg::Style { color: Some(color) }
        }))
}

pub fn fill_container(color: Color, radius: f32) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| cosmic::widget::container::Style {
        background: Some(Background::Color(color)),
        border: Border {
            radius: radius.into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

pub fn bordered_fill_container(
    background: Color,
    border_color: Color,
    radius: f32,
) -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(move |_| cosmic::widget::container::Style {
        background: Some(Background::Color(background)),
        border: Border {
            color: border_color,
            radius: radius.into(),
            width: 1.0,
        },
        ..Default::default()
    })
}

pub fn context_menu_container() -> cosmic::theme::Container<'static> {
    cosmic::theme::Container::custom(|_| cosmic::widget::container::Style {
        background: Some(Background::Color(style::CARD_BG)),
        border: Border {
            color: style::DIVIDER,
            radius: style::RADIUS_CARD.into(),
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

pub fn divider<Message: 'static>() -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fill)
        .height(Length::Fixed(style::DIVIDER_HEIGHT))
        .class(fill_container(style::DIVIDER, 0.0))
        .into()
}

pub fn inset_divider<Message: 'static>(padding: u16) -> Element<'static, Message> {
    container(divider())
        .width(Length::Fill)
        .height(Length::Fixed(style::DIVIDER_HEIGHT))
        .padding([0, padding])
        .into()
}

pub fn dot<Message: 'static>(color: Color, size: f32) -> Element<'static, Message> {
    container(horizontal_space())
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .class(fill_container(color, size / 2.0))
        .into()
}

pub fn card<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    height: f32,
) -> Element<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .class(fill_container(style::CARD_BG, style::RADIUS_CARD))
        .into()
}

pub fn card_container<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> container::Container<'a, Message, cosmic::Theme> {
    container(content)
        .width(Length::Fill)
        .class(fill_container(style::CARD_BG, style::RADIUS_CARD))
}

pub fn pill_button_style(background: Color, text_color: Color) -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(move |focused, _| button_appearance(background, text_color, focused)),
        disabled: Box::new(move |_| {
            let mut disabled_background = background;
            disabled_background.a = 0.45;
            let mut disabled_text = text_color;
            disabled_text.a = 0.55;
            button_appearance(disabled_background, disabled_text, false)
        }),
        hovered: Box::new(move |focused, _| button_appearance(background, text_color, focused)),
        pressed: Box::new(move |focused, _| button_appearance(background, text_color, focused)),
    }
}

pub fn button_appearance(
    background: Color,
    text_color: Color,
    focused: bool,
) -> cosmic::widget::button::Style {
    cosmic::widget::button::Style {
        background: Some(Background::Color(background)),
        border_radius: style::RADIUS_PILL.into(),
        border_width: if focused { 1.0 } else { 0.0 },
        border_color: if focused {
            style::ACCENT
        } else {
            Color::TRANSPARENT
        },
        text_color: Some(text_color),
        icon_color: Some(text_color),
        outline_width: if focused { 1.0 } else { 0.0 },
        outline_color: style::ACCENT,
        ..Default::default()
    }
}

pub fn dropdown_action<Message: Clone + Send + Sync + 'static>(
    selected_label: String,
    labels: Vec<String>,
    selected: Option<usize>,
    open: bool,
    toggle: fn(bool) -> Message,
    select: impl Fn(usize) -> Message + Send + Sync + 'static,
    widths: DropdownWidths,
) -> Element<'static, Message> {
    let trigger = button::custom(
        row::with_capacity(2)
            .push(
                text::body(selected_label)
                    .size(14)
                    .class(style::BODY_TEXT)
                    .width(Length::Fill)
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
            )
            .push(symbolic_icon(
                "pan-down-symbolic",
                style::ICON_SIZE,
                style::BODY_TEXT,
            ))
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .spacing(4),
    )
    .padding(8)
    .width(Length::Fixed(widths.trigger))
    .height(Length::Fixed(37.0))
    .class(cosmic::theme::Button::Transparent)
    .on_press(toggle(!open));

    if open {
        widget::popover(trigger)
            .position(widget::popover::Position::Bottom)
            .popup(dropdown_menu(labels, selected, select, widths.popup))
            .on_close(toggle(false))
            .into()
    } else {
        trigger.into()
    }
}

fn dropdown_menu<Message: Clone + Send + Sync + 'static>(
    labels: Vec<String>,
    selected: Option<usize>,
    select: impl Fn(usize) -> Message + Send + Sync + 'static,
    width: f32,
) -> Element<'static, Message> {
    let mut menu = column::with_capacity(labels.len())
        .padding([8, 0])
        .width(Length::Fixed(width));
    let select = Arc::new(select);

    for (index, label) in labels.into_iter().enumerate() {
        let selected_row = selected == Some(index);
        let check: Element<'static, Message> = if selected_row {
            widget::icon::from_name("object-select-symbolic")
                .size(style::ICON_SIZE)
                .into()
        } else {
            horizontal_space().width(Length::Fixed(16.0)).into()
        };
        let row = row::with_capacity(2)
            .push(
                text::body(label)
                    .size(14)
                    .class(if selected_row {
                        style::ACCENT
                    } else {
                        style::BODY_TEXT
                    })
                    .width(Length::Fill)
                    .wrapping(Wrapping::None)
                    .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1))),
            )
            .push(check)
            .align_y(Alignment::Center)
            .spacing(8);
        let select = Arc::clone(&select);

        menu = menu.push(
            button::custom(row)
                .width(Length::Fill)
                .padding([4, 12])
                .class(cosmic::theme::Button::Transparent)
                .on_press(select(index)),
        );
    }

    container(menu)
        .class(cosmic::theme::Container::Dropdown)
        .into()
}
