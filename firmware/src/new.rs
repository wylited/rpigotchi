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
use std::thread;
use std::time::Duration;
use thiserror::Error;

mod utils; // Assuming your draw_text is here
use utils::draw_text;

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
    epd: Epd2in13<SysfsPin, SysfsPin, SysfsPin, Delay>,
    display: Display2in13,
    delay: Delay,
    // Keep pins for proper cleanup
    cs: SysfsPin,
    busy: SysfsPin,
    dc: SysfsPin,
    rst: SysfsPin,
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

        let display = Display2in13::default();

        Ok(EpaperApp {
            spi,
            epd,
            display,
            delay,
            cs,
            busy,
            dc,
            rst,
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
        self.test_graphics()?;
        self.test_spinner()?;
        Ok(())
    }

    fn test_graphics(&mut self) -> Result<(), EpaperError> {
        println!("Testing graphics...");

        self.display.clear(Color::White).ok();

        // Draw analog clock
        Circle::with_center(Point::new(64, 64), 80)
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut self.display)
            .map_err(|_| EpaperError::DisplayInit)?;

        Line::new(Point::new(64, 64), Point::new(30, 40))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 4))
            .draw(&mut self.display)
            .map_err(|_| EpaperError::DisplayInit)?;

        Line::new(Point::new(64, 64), Point::new(80, 40))
            .into_styled(PrimitiveStyle::with_stroke(Color::Black, 1))
            .draw(&mut self.display)
            .map_err(|_| EpaperError::DisplayInit)?;

        // Bigger font
        let style = MonoTextStyleBuilder::new()
            .font(&embedded_graphics::mono_font::ascii::FONT_10X20)
            .text_color(Color::White)
            .background_color(Color::Black)
            .build();

        Text::with_text_style("It's working\nWoB!", Point::new(90, 40), style, text_style)
            .draw(&mut self.display)
            .map_err(|_| EpaperError::DisplayInit)?;

        Ok(())
    }

    fn test_spinner(&mut self) -> Result<(), EpaperError> {
        println!("Testing spinner...");

        self.display.clear(Color::White).ok();
        self.epd
            .update_and_display_frame(&mut self.spi, self.display.buffer(), &mut self.delay)?;

        let spinner = ["|", "/", "-", "\\"];
        for i in 0..10 {
            self.display.clear(Color::White).ok();
            draw_text(&mut self.display, spinner[i % spinner.len()], 10, 100);
            self.epd.update_and_display_frame(
                &mut self.spi,
                self.display.buffer(),
                &mut self.delay,
            )?;
        }

        Ok(())
    }

    pub fn shutdown(mut self) -> Result<(), EpaperError> {
        println!("Shutting down display...");
        self.epd.sleep(&mut self.spi, &mut self.delay)?;

        // Clean up GPIO pins
        self.cs.unexport().ok();
        self.busy.unexport().ok();
        self.dc.unexport().ok();
        self.rst.unexport().ok();

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

// Example of running in a thread
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
    // You can run it directly
    // run_epaper_app()?;

    // Or in a thread
    run_epaper_threaded()?;

    println!("Finished tests");
    Ok(())
}
