#![no_main]
#![no_std]

use {defmt_rtt as _, panic_probe as _};

use stm32f4xx_hal as hal;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use stm32f4xx_hal::gpio::{ErasedPin, Output};
use stm32f4xx_hal::timer::SysDelay;

#[entry]
fn main() -> ! {
    let core = cortex_m::Peripherals::take().unwrap();
    let p = pac::Peripherals::take().unwrap();


    let rcc = p.RCC.constrain();
    let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

    let gpioc = p.GPIOC.split();
    let mut led = gpioc.pc13.into_push_pull_output().erase();

    let mut delay = core.SYST.delay(&clocks);

    let table = [
        ('A', ".-"),
        ('B', "-..."),
        ('C', "-.-."),
        ('D', "-.."),
        ('E', "."),
        ('F', "..-."),
        ('G', "--."),
        ('H', "...."),
        ('I', ".."),
        ('J', ".---"),
        ('K', "-.-"),
        ('L', ".-.."),
        ('M', "--"),
        ('N', "-."),
        ('O', "---"),
        ('P', ".--."),
        ('Q', "--.-"),
        ('R', ".-."),
        ('S', "..."),
        ('T', "-"),
        ('U', "..-"),
        ('V', "...-"),
        ('W', ".--"),
        ('X', "-..-"),
        ('Y', "-.--"),
        ('Z', "--.."),
    ];

    let message = "SOS";

    loop {
        for m in message.chars() {
            if m == ' ' {
                delay.delay_ms(700_u32);
            } else {
                for (c, s) in table {
                    if m == c {
                        symbol(s, &mut led, &mut delay);
                    }
                }
            }
        }
        delay.delay_ms(3000_u32);
    }
}

fn symbol(s: &str, led: &mut ErasedPin<Output>, delay: &mut SysDelay) {
    for c in s.chars() {
        if c == '.' {
            dot(led, delay);
        } else if c == '-' {
            dash(led, delay);
        }
    }
    delay.delay_ms(300_u32);
}

fn dot(led: &mut ErasedPin<Output>, delay: &mut SysDelay) {
    led.set_low();
    delay.delay_ms(100_u32);
    led.set_high();
    delay.delay_ms(100_u32);
}

fn dash(led: &mut ErasedPin<Output>, delay: &mut SysDelay) {
    led.set_low();
    delay.delay_ms(300_u32);
    led.set_high();
    delay.delay_ms(100_u32);
}