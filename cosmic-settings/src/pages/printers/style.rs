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
pub const QUEUE_SELECTION_BG: Color = Color::from_rgba(
    0x4D as f32 / 255.0,
    0x4D as f32 / 255.0,
    0x4D as f32 / 255.0,
    0.3,
);

// Supply and destructive-action colors.
pub const SUPPLY_TRACK: Color = Color::from_rgb(
    0x63 as f32 / 255.0,
    0x63 as f32 / 255.0,
    0x63 as f32 / 255.0,
);
/// How bright a supply's strongest channel has to be to be seen on the card.
///
/// A black cartridge reports itself as black, which on a dark card is no bar at all,
/// so a colour below this is lifted to reach it.
pub const SUPPLY_MIN_CHANNEL: f32 = 0x9A as f32 / 255.0;
/// What a supply is drawn in when it has no colour of its own to lift.
pub const SUPPLY_NEUTRAL: Color =
    Color::from_rgb(SUPPLY_MIN_CHANNEL, SUPPLY_MIN_CHANNEL, SUPPLY_MIN_CHANNEL);
/// How close to the track a colour may be before it needs an edge to be told apart.
pub const SUPPLY_OUTLINE_TOLERANCE: f32 = 0.15;
pub const REMOVE_BG: Color = Color::from_rgb(
    0xFF as f32 / 255.0,
    0xA0 as f32 / 255.0,
    0x9A as f32 / 255.0,
);
pub const REMOVE_TEXT: Color = Color::BLACK;

// Supply card dimensions, from the design.
/// A label of 21 and a bar row of 20, stacked with no gap.
pub const SUPPLY_GRAPH_HEIGHT: f32 = 41.0;
pub const SUPPLY_LABEL_HEIGHT: f32 = 21.0;
pub const SUPPLY_BAR_HEIGHT: f32 = 20.0;
pub const SUPPLY_TRACK_HEIGHT: f32 = 12.0;
pub const SUPPLY_CARD_PADDING_Y: f32 = 8.0;
pub const SUPPLY_COLUMN_SPACING: u16 = 16;
pub const SUPPLY_ROW_SPACING: u16 = 12;
pub const SUPPLY_PERCENTAGE_WIDTH: f32 = 48.0;
pub const SUPPLY_DOT_SIZE: f32 = 8.0;
/// The mark stands taller than the bar so it reads over whatever is under it.
pub const SUPPLY_MARK_WIDTH: f32 = 2.0;
pub const SUPPLY_MARK_HEIGHT: f32 = 16.0;

// Shared dimensions.
pub const RADIUS_CARD: f32 = 8.0;
pub const RADIUS_SUPPLY_BAR: f32 = 40.0;
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
