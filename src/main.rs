#![no_main]
#![no_std]

use {defmt_rtt as _, panic_probe as _};

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Line, PrimitiveStyle, StyledDrawable};
use ssd1306::{I2CDisplayInterface, Ssd1306};
use ssd1306::prelude::*;
use stm32f4xx_hal::block;
use stm32f4xx_hal::i2c::Mode;
use stm32f4xx_hal::serial::Config;
use u8g2_fonts::{fonts, FontRenderer};
use u8g2_fonts::types::{FontColor, HorizontalAlignment, VerticalPosition};

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();

    let mut rcc = p.RCC.freeze(hal::rcc::Config::default());

    let gpioa = p.GPIOA.split(&mut rcc);
    let gpiob = p.GPIOB.split(&mut rcc);
    let gpioc = p.GPIOC.split(&mut rcc);
    let mut led = gpioc.pc13.into_push_pull_output();

    let mut delay = core.SYST.delay(&rcc.clocks);

    let tx = gpiob.pb6;
    let rx = gpiob.pb7;

    let mut serial = p.USART1.serial::<u8>((tx, rx), Config::default(), &mut rcc).unwrap();

    let scl = gpiob.pb8;
    let sda = gpiob.pb9;

    let i2c = p.I2C1.i2c((scl, sda), Mode::Standard {
        frequency: 100.kHz(),
    }, &mut rcc);

    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306::new(
        interface,
        DisplaySize96x16,
        DisplayRotation::Rotate0,
    ).into_buffered_graphics_mode();

    display.init().unwrap();
    display.clear(BinaryColor::Off).unwrap();
    display.flush().unwrap();

    let font = FontRenderer::new::<fonts::u8g2_font_haxrcorp4089_t_cyrillic>();
    let text = "Привет мир!";

    let draw_area = display.size();
    defmt::info!("Display size: {}", draw_area);

    font.render_aligned(
        text,
        Point::new(0, 0),
        VerticalPosition::Top,
        HorizontalAlignment::Left,
        FontColor::Transparent(BinaryColor::On),
        &mut display,
    )
        .unwrap();

    display.flush().unwrap();

    loop {
        let progress = 0.5;
        let full_line = Line::new(
            Point::new(0, (draw_area.height - 4) as i32),
            Point::new(draw_area.width as i32, draw_area.height as i32)
        );
        let part_line = Line::new(
            Point::new(0, (draw_area.height - 4) as i32),
            Point::new(((draw_area.width as f32) * progress) as i32, draw_area.height as i32)
        );

        // full_line.draw_styled(&PrimitiveStyle::with_fill(BinaryColor::Off), &mut display).unwrap();
        part_line.draw_styled(&PrimitiveStyle::with_fill(BinaryColor::On), &mut display).unwrap();
    }
}
