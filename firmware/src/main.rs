use chrono::Local;
use embedded_graphics::{
    mono_font::MonoTextStyleBuilder,
    prelude::*,
    primitives::{Circle, Line, PrimitiveStyle},
    text::{Baseline, Text, TextStyleBuilder},
};
use embedded_hal::delay::DelayNs;
use epd_waveshare::{
    color::*,
    epd2in13_v2::{Display2in13, Epd2in13},
    graphics::DisplayRotation,
    prelude::*,
};
use linux_embedded_hal::{
    spidev::{self, SpidevOptions},
    sysfs_gpio::Direction,
    Delay, SPIError, SpidevDevice, SysfsPin,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use thiserror::Error;

mod utils;
use utils::draw_text;
mod spotify;

#[derive(Error, Debug)]
pub enum EpaperError {
    #[error("SPI error: {0}")]
    Spi(#[from] SPIError),
    #[error("GPIO error: {0}")]
    Gpio(#[from] linux_embedded_hal::sysfs_gpio::Error),
    #[error("Display initialization error")]
    DisplayInit,
    #[error("Pin export timeout")]
    PinExportTimeout,
}

pub struct EpaperApp {
    spi: SpidevDevice,
    epd: Epd2in13<SpidevDevice, SysfsPin, SysfsPin, SysfsPin, Delay>,
    display: Display2in13,
    delay: Delay,
    // keep pins for proper cleanup
    // cs: SysfsPin,
    // busy: SysfsPin,
    // dc: SysfsPin,
    // rst: SysfsPin,
    // but do I really need
}

impl EpaperApp {
    pub fn new() -> Result<Self, EpaperError> {
        // configure SPI setup
        let mut spi = SpidevDevice::open("/dev/spidev0.0").map_err(|_| EpaperError::DisplayInit)?;

        let options = SpidevOptions::new()
            .bits_per_word(8)
            .max_speed_hz(4_000_000)
            .mode(spidev::SpiModeFlags::SPI_MODE_0)
            .build();

        spi.configure(&options)
            .map_err(|_| EpaperError::DisplayInit)?;

        // setup GPIO pins with proper timing idk
        let cs = Self::setup_output_pin(26, 1)?;
        let busy = Self::setup_input_pin(24)?;
        let dc = Self::setup_output_pin(25, 1)?;
        let rst = Self::setup_output_pin(17, 1)?;

        let mut delay = Delay {};

        // init e-paper display
        let epd = Epd2in13::new(&mut spi, busy, dc, rst, &mut delay, None)
            .map_err(|_| EpaperError::DisplayInit)?;

        let mut display = Display2in13::default();
        display.set_rotation(DisplayRotation::Rotate270);

        Ok(EpaperApp {
            spi,
            epd,
            display,
            delay,
            // cs,
            // busy,
            // dc,
            // rst,
        })
    }

    fn setup_output_pin(pin_num: u64, initial_value: u8) -> Result<SysfsPin, EpaperError> {
        let pin = SysfsPin::new(pin_num);
        pin.export()?;

        // wait for export with timeout ()#5)
        let timeout = Duration::from_millis(100);
        let start = std::time::Instant::now();

        while !pin.is_exported() {
            if start.elapsed() > timeout {
                return Err(EpaperError::PinExportTimeout);
            }
            thread::sleep(Duration::from_millis(5));
        }

        pin.set_direction(Direction::Out)?;
        pin.set_value(initial_value)?;
        Ok(pin)
    }

    fn setup_input_pin(pin_num: u64) -> Result<SysfsPin, EpaperError> {
        let pin = SysfsPin::new(pin_num);
        pin.export()?;

        // wait for export with timeout (#5)
        let timeout = Duration::from_millis(100);
        let start = std::time::Instant::now();

        while !pin.is_exported() {
            if start.elapsed() > timeout {
                return Err(EpaperError::PinExportTimeout);
            }
            thread::sleep(Duration::from_millis(5));
        }

        pin.set_direction(Direction::In)?;
        Ok(pin)
    }

    pub fn run(&mut self) -> Result<(), EpaperError> {
        // Setup a handler for Ctrl+C
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();

        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
            println!("Received Ctrl+C, shutting down...");
        })
        .expect("Error setting Ctrl+C handler");

        self.display.clear(Color::White).ok();
        self.epd
            .update_and_display_frame(&mut self.spi, self.display.buffer(), &mut self.delay)?;

        // Define spinner characters - using larger ones for visibility
        let spinner = ["|", "/", "-", "\\"];
        let mut i = 0;

        println!("Running spinner. Press Ctrl+C to exit...");
        self.epd
            .set_refresh(&mut self.spi, &mut self.delay, RefreshLut::Quick)
            .unwrap();

        self.epd
            .clear_frame(&mut self.spi, &mut self.delay)
            .unwrap();

        while running.load(Ordering::SeqCst) {
            self.display.clear(Color::White).ok();

            // Draw a large spinner in the center of the display
            // Using the built-in draw_text utility
            let spinner_char = spinner[i % spinner.len()];

            // Draw a large spinner text in the center
            let text_style = TextStyleBuilder::new().baseline(Baseline::Top).build();
            let style = MonoTextStyleBuilder::new()
                .font(&embedded_graphics::mono_font::ascii::FONT_10X20)
                .text_color(Color::Black)
                .background_color(Color::White)
                .build();

            Text::with_text_style(
                spinner_char,
                Point::new(250 / 2, 122 / 2),
                style,
                text_style,
            )
            .draw(&mut self.display)
            .map_err(|_| EpaperError::DisplayInit)?;

            // draw text indicating how to exit
            draw_text(&mut self.display, "Press Ctrl+C to exit", 0, 112);

            let now = Local::now();
            let time_str = now.format("%H:%M:%S").to_string();

            // draw the time text
            draw_text(
                &mut self.display,
                &time_str,
                250 - (time_str.len() as i32 * 10),
                112,
            );

            // update the display
            self.epd.update_and_display_frame(
                &mut self.spi,
                self.display.buffer(),
                &mut self.delay,
            )?;

            // move to next spinner frame
            i = (i + 1) % spinner.len();

            // short delay between frames
            thread::sleep(Duration::from_millis(500));
        }

        Ok(())
    }

    pub fn shutdown(mut self) -> Result<(), EpaperError> {
        println!("Shutting down display...");
        self.epd.sleep(&mut self.spi, &mut self.delay)?;

        // Clean up GPIO pins
        // self.cs.unexport().ok();
        // self.busy.unexport().ok();
        // self.dc.unexport().ok();
        // self.rst.unexport().ok();
        // Do I really need to clean up

        Ok(())
    }
}

// For threading support
unsafe impl Send for EpaperApp {}

pub fn run_epaper_app() -> Result<(), EpaperError> {
    let mut app = EpaperApp::new()?;
    app.run()?;
    app.shutdown()?;
    Ok(())
}

pub fn run_epaper_threaded() -> Result<(), EpaperError> {
    let handle = thread::spawn(|| -> Result<(), EpaperError> {
        let mut app = EpaperApp::new()?;
        app.run()?;
        app.shutdown()?;
        Ok(())
    });

    handle.join().map_err(|_| EpaperError::DisplayInit)??;
    Ok(())
}

fn main() -> Result<(), EpaperError> {
    run_epaper_app()?;
    // Or in a thread
    // run_epaper_threaded()?;

    println!("Finished tests");
    Ok(())
}
