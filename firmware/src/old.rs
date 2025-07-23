use anyhow::Result;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    prelude::*,
    text::{Baseline, Text, TextStyleBuilder},
};
use epd_waveshare::{
    color::*,
    epd2in13_v2::{Display2in13, Epd2in13},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    spidev::{self, SpidevOptions},
    sysfs_gpio::Direction,
    Delay, SpidevDevice, SysfsPin
};
use std::{thread, time::Duration};
use tokio::net::TcpListener;
use tokio_tungstenite::accept_async;
use futures_util::{SinkExt, StreamExt};

/// Pin numbers in BCM notation
const SPI_DEV: &str = "/dev/spidev0.0";
const BUSY: u64 = 17; // GPIO 17 – pin 11
const DC: u64   = 25; // GPIO 25 – pin 22
const RST: u64  = 27; // GPIO 27 – pin 13
const CS: u64   = 5;  // GPIO 8  – pin 24 (CE0)

fn main() -> Result<()> {
    // --- 1. UI thread --------------------------------------------------------
    thread::spawn(|| {
        if let Err(e) = eink_hello() {
            eprintln!("e-Ink error: {e}");
        }
    });

    // --- 2. WebSocket thread -------------------------------------------------
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(websocket_echo())?;
    Ok(())
}

/// Draw “Hello Rust” once, then keep the display asleep.
fn eink_hello() -> Result<()> {
    // SPI
    let mut spi = SpidevDevice::open(SPI_DEV)?;
    spi.configure(
        &SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(4_000_000)
            .mode(spidev::SpiModeFlags::SPI_MODE_0)
            //.lsb_first(true)
            .build(),
    )?;

    // GPIO
    let busy = export_pin(BUSY, Direction::In)?;
    let dc   = export_pin(DC,   Direction::Out)?;
    let rst  = export_pin(RST,  Direction::Out)?;
    let _cs  = export_pin(CS,   Direction::Out)?;

    let mut delay = Delay {};

    // Initialize display
    let mut epd = Epd2in13::new(&mut spi, busy, dc, rst, &mut delay, None)?;
    let mut disp = Display2in13::default();
    disp.set_rotation(DisplayRotation::Rotate0);
    disp.clear(Color::White)?;

    // Draw text
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_6X10)
        .text_color(Color::Black)
        .background_color(Color::White)
        .build();
    let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
    Text::with_text_style("Hello Rust", Point::new(30, 100), style, text_style)
        .draw(&mut disp)?;

    println!("sleeping now");

    // Update and sleep
    epd.update_frame(&mut spi, disp.buffer(), &mut delay)?;
    epd.display_frame(&mut spi, &mut delay)?;
    epd.sleep(&mut spi, &mut delay)?;

    Ok(())
}

/// Export + wait 100 ms to avoid permission race
fn export_pin(pin_num: u64, dir: Direction) -> Result<SysfsPin> {
    let pin = SysfsPin::new(pin_num);
    pin.export()?;
    while !pin.is_exported() {
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(100)); // <-- GitHub issue fix
    pin.set_direction(dir)?;
    Ok(pin)
}

/// Echo every text message received on :9001
async fn websocket_echo() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:9001").await?;
    println!("WebSocket listening on ws://0.0.0.0:9001");

    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut ws = accept_async(stream).await.expect("handshake");
            while let Some(Ok(msg)) = ws.next().await {
                if msg.is_text() {
                    let txt = msg.to_text().unwrap_or("<invalid>");
                    println!("WS: {}", txt);
                }
            }
        });
    }
    Ok(())
}
