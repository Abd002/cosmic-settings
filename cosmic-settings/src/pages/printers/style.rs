use cosmic::iced::Color;

// General page colors.
pub const ACCENT: Color = Color::from_rgb(
    0x63 as f32 / 255.0,
    0xD0 as f32 / 255.0,
    0xDF as f32 / 255.0,
);
pub const CARD_BG: Color = Color::from_rgb(
    0x2E as f32 / 255.0,
    0x2E as f32 / 255.0,
    0x2E as f32 / 255.0,
);
pub const BODY_TEXT: Color = Color::from_rgb(
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
);
pub const SECONDARY_TEXT: Color = Color::from_rgba(
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
    0.75,
);
pub const TITLE_TEXT: Color = Color::from_rgb(
    0xC4 as f32 / 255.0,
    0xC4 as f32 / 255.0,
    0xC4 as f32 / 255.0,
);
pub const DIVIDER: Color = Color::from_rgba(
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
    0xDE as f32 / 255.0,
    0.2,
);

// Printer status colors.
pub const STATUS_READY: Color = Color::from_rgb(
    0x5E as f32 / 255.0,
    0xDB as f32 / 255.0,
    0x8C as f32 / 255.0,
);
pub const STATUS_PRINTING: Color = ACCENT;
pub const STATUS_STOPPED: Color = Color::from_rgb(
    0xFF as f32 / 255.0,
    0xA3 as f32 / 255.0,
    0x7D as f32 / 255.0,
);

// Dialog and neutral widget colors.
pub const NEUTRAL_WIDGET_BG: Color = Color::from_rgb(
    0x26 as f32 / 255.0,
    0x26 as f32 / 255.0,
    0x26 as f32 / 255.0,
);
pub const DARK_DIALOG: Color = Color::from_rgb(0.106, 0.106, 0.106);
pub const DARK_LIST: Color = Color::from_rgb(0.180, 0.180, 0.180);
pub const DARK_FOOTER: Color = Color::from_rgb(0.149, 0.149, 0.149);
pub const BORDER_SUBTLE: Color = Color::from_rgba(0.769, 0.769, 0.769, 0.2);
pub const TEXT_MUTED: Color = Color::from_rgb(0.769, 0.769, 0.769);
pub const BUTTON_CANCEL: Color = Color::from_rgb(0.200, 0.200, 0.200);

// Queue colors.
pub const QUEUE_LIST_BG: Color = Color::from_rgb(
    0x35 as f32 / 255.0,
    0x35 as f32 / 255.0,
    0x35 as f32 / 255.0,
);
pub const QUEUE_FOREGROUND: Color = Color::from_rgb(
    0xE8 as f32 / 255.0,
    0xE8 as f32 / 255.0,
    0xE8 as f32 / 255.0,
);
pub const QUEUE_ERROR: Color = Color::from_rgb(
    0xFF as f32 / 255.0,
    0xA0 as f32 / 255.0,
    0x9A as f32 / 255.0,
);

// Supply and destructive-action colors.
pub const SUPPLY_TRACK: Color = Color::from_rgb(
    0x63 as f32 / 255.0,
    0x63 as f32 / 255.0,
    0x63 as f32 / 255.0,
);
pub const BLACK_SUPPLY: Color = Color::from_rgb(
    0x80 as f32 / 255.0,
    0x80 as f32 / 255.0,
    0x80 as f32 / 255.0,
);
pub const REMOVE_BG: Color = Color::from_rgb(
    0xFF as f32 / 255.0,
    0xA0 as f32 / 255.0,
    0x9A as f32 / 255.0,
);
pub const REMOVE_TEXT: Color = Color::BLACK;

// Shared dimensions.
pub const RADIUS_CARD: f32 = 8.0;
pub const RADIUS_PILL: f32 = 160.0;
pub const DIVIDER_HEIGHT: f32 = 1.0;
pub const ICON_SIZE: u16 = 16;

pub const FONT_SEMIBOLD: cosmic::iced::Font = cosmic::iced::Font {
    weight: cosmic::iced::font::Weight::Semibold,
    ..cosmic::iced::Font::DEFAULT
};
pub const FONT_BOLD: cosmic::iced::Font = cosmic::iced::Font {
    weight: cosmic::iced::font::Weight::Bold,
    ..cosmic::iced::Font::DEFAULT
};
