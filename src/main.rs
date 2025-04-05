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

    let fpu_cpacr = peripherals.FPU_CPACR;
    fpu_cpacr.cpacr.write(|w| unsafe { w.cp().bits(0b1111) });
    let rcc = peripherals.RCC;
    rcc.cr.modify(|r, w|
        w
            .hsion().set_bit()
            .hseon().clear_bit()
            .csson().clear_bit()
            .pllon().clear_bit()
    );
    rcc.pllcfgr.modify(|r, w|
        unsafe {
            w
                .bits(0b100100000000000011000000010000)

        }
    );
    rcc.cr.modify(|r, w|
        w
            .hsebyp().clear_bit()
    );
    rcc.cir.modify(|r, w|
        unsafe {
            w.bits(0)
        }
    );

    let rcc = rcc.constrain();
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
