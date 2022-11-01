#![no_std]
#![no_main]

extern crate stm32f4xx_hal as hal;
extern crate cortex_m_rt as rt;
extern crate panic_semihosting;

use hal::gpio::{GpioExt, Pin};
use hal::prelude::*;

#[rt::entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let peripherals = hal::pac::Peripherals::take().unwrap();

    let rcc = peripherals.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

    let bus_c = peripherals.GPIOC.split();
    let mut led: Pin<'C', 13, _> = bus_c.pc13.into_push_pull_output();

    let mut delay = core.SYST.delay(&clocks);

    loop {
        led.set_low();
        // delay.delay_ms(100u32);
        led.set_high();
        delay.delay_ms(1000u32);
    }
}
