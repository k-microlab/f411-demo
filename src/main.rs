#![no_main]
#![no_std]

use panic_halt as _;

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();


    let rcc = p.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

    let gpioc = p.GPIOC.split();
    let mut led = gpioc.pc13.into_push_pull_output();

    let mut delay = core.SYST.delay(&clocks);

    loop {
        led.set_low();
        delay.delay_ms(100u32);
        led.set_high();
        delay.delay_ms(1000u32);
    }
}
