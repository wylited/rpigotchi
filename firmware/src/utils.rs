use embedded_graphics::{
    mono_font::MonoTextStyleBuilder,
    prelude::Point,
    text::{Baseline, Text, TextStyleBuilder},
    Drawable,
};
use epd_waveshare::{color::Color, epd2in13_v2::Display2in13};

pub fn draw_text(display: &mut Display2in13, text: &str, x: i32, y: i32) {
    let style = MonoTextStyleBuilder::new()
        .font(&embedded_graphics::mono_font::ascii::FONT_6X10)
        .text_color(Color::White)
        .background_color(Color::Black)
        .build();

    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();

    let _ = Text::with_text_style(text, Point::new(x, y), style, text_style).draw(display);
}
