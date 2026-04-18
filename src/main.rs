#![no_main]
#![no_std]

use {defmt_rtt as _, panic_probe as _};

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use defmt::{error, info};
use stm32f4xx_hal::adc::Adc;
use stm32f4xx_hal::adc::config::AdcConfig;
use stm32f4xx_hal::i2c::{DutyCycle, Mode};
use stm32f4xx_hal::rcc::{Clocks, Config};
use stm32f4xx_hal::serial::{Serial};

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use embedded_graphics::mono_font::MonoTextStyle;
use panic_probe as _;
use ssd1306::{prelude::*, I2CDisplayInterface, Ssd1306};

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();

    info!("Device init");

    let mut rcc = p.RCC.constrain().freeze(Config::default().sysclk(48.MHz()));

    let gpioa = p.GPIOA.split(&mut rcc);
    let gpiob = p.GPIOB.split(&mut rcc);

    let mut delay = core.SYST.delay(&rcc.clocks);

    let mut adc = Adc::new(p.ADC1, true, AdcConfig::default(), &mut rcc);
    let mut pa0 = gpioa.pa0.into_analog();

    // let mut serial = p.USART1.serial::<_, _, u8>((gpiob.pb6, gpiob.pb7), Config::default(), &clocks).unwrap();

    let i2c = p.I2C1.i2c((gpiob.pb8, gpiob.pb7), Mode::Fast {
        frequency: 400.kHz(),
        duty_cycle: DutyCycle::Ratio2to1,
    }, &mut rcc);

    let display = I2CDisplayInterface::new(i2c);

    let mut lcd = Ssd1306::new(display, DisplaySize96x16, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();

    if let Some(e)  = lcd.init().err() {
        error!("Failed to init SD1306 interface: {:?}", e);
    }

    let text_style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let text = Text::new("Hello, world!", Point::new(10, 13), text_style);

    text.draw(&mut lcd).unwrap();

    if let Some(e) = lcd.flush().err() {
        error!("Failed to flush display: {}", e);
    }

    loop {

    }
}
