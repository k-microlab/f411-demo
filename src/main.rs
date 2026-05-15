#![no_main]
#![no_std]

use {defmt_rtt as _, panic_probe as _};

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use ssd1306::{I2CDisplayInterface, Ssd1306};
use ssd1306::prelude::*;
use stm32f4xx_hal::block;
use stm32f4xx_hal::i2c::Mode;
use stm32f4xx_hal::serial::Config;

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();


    let rcc = p.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

    let gpioa = p.GPIOA.split();
    let gpiob = p.GPIOB.split();
    let gpioc = p.GPIOC.split();
    let mut led = gpioc.pc13.into_push_pull_output();

    let mut delay = core.SYST.delay(&clocks);

    let tx = gpiob.pb6;
    let rx = gpiob.pb7;

    let mut serial = p.USART1.serial::<_, _, u8>((tx, rx), Config::default(), &clocks).unwrap();

    let scl = gpiob.pb8;
    let sda = gpiob.pb9;

    let i2c = p.I2C1.i2c((scl, sda), Mode::Standard {
        frequency: 100.kHz(),
    }, &clocks);

    let interface = I2CDisplayInterface::new(i2c);

    let mut display = Ssd1306::new(
        interface,
        DisplaySize96x16,
        DisplayRotation::Rotate0,
    ).into_terminal_mode();

    display.clear().unwrap();
    display.init().unwrap();

    let mut row = 0;
    let mut column = 0;

    loop {
        let byte = block!(serial.read()).unwrap();
        let c = byte as char;
        defmt::info!("Received character: {}", c);

        if c.is_ascii_alphanumeric() || c.is_ascii_punctuation() || c == ' ' {
            column += 1;
            display.print_char(c).unwrap();
        } else if c == '\r' {
            row += 1;
            if row >= 2 {
                row = 0;
            }
            display.set_row(row * 8).unwrap();
            display.set_column(0).unwrap();
        } else if c == '\x7f' {
            display.set_column(column).unwrap();
            display.print_char(' ').unwrap();
            if column > 0 {
                column -= 1;
            }
        }
    }
}
