#![no_main]
#![no_std]

use core::fmt::Write;
use {defmt_rtt as _, panic_probe as _};

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use stm32f4xx_hal::block;
use stm32f4xx_hal::serial::Config;

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();


    let rcc = p.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

    let gpioc = p.GPIOC.split();
    let gpiob = p.GPIOB.split();
    let mut led = gpioc.pc13.into_push_pull_output();

    let mut delay = core.SYST.delay(&clocks);

    let tx = gpiob.pb6;
    let rx = gpiob.pb7;

    let mut serial = p.USART1.serial::<_, _, u8>((tx, rx), Config::default(), &clocks).unwrap();

    let mut array: [u8; 256] = [0; 256];
    let mut len = 0;

    loop {
        let byte = block!(serial.read()).unwrap();
        let _ = block!(serial.write(byte));
        if byte == b'\x7f' { //Backspace
            if len > 0 {
                len -= 1;
            }
        } else if byte == b'\r' {
            let s = str::from_utf8(&array[0..len]).unwrap();
            len = 0;
            serial.write(b'\r').unwrap(); // New line
            serial.write(b'\n').unwrap(); // New line

            if s == "led on" {
                led.set_low();
                serial.write_str("Turned on LED\n").unwrap();
            } else if s == "led off" {
                led.set_high();
                serial.write_str("Turned off LED\n").unwrap();
            }
        } else if byte == b'\n' {
            // defmt::info!("New line: {:?}", &array[0..len]);
        } else {
            let c = byte as char;
            defmt::info!("Received character: {:?}", c);
            array[len] = byte;
            len += 1;
        }

    }
}
