#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::mode::Async;
use embassy_stm32::{bind_interrupts, exti, interrupt, spi};
use embassy_stm32::spi::mode::Master;
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    EXTI2 => exti::InterruptHandler<interrupt::typelevel::EXTI2>;
    EXTI4 => exti::InterruptHandler<interrupt::typelevel::EXTI4>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let config = Default::default();
    let p = embassy_stm32::init(config);

    let cs = Output::new(p.PB0, Level::High, Speed::Medium);
    let reset = Output::new(p.PB1, Level::High, Speed::Medium);
    let busy = ExtiInput::new(p.PB2, p.EXTI2, Pull::Down, Irqs);
    let dio1 = ExtiInput::new(p.PA4, p.EXTI4, Pull::Down, Irqs);
    let dio2 = Input::new(p.PA3, Pull::Down);
    let dio3 = Input::new(p.PA2, Pull::Down);
    let dio4 = Input::new(p.PA1, Pull::Down);

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(1_000_000);
    let spi = Spi::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA2_CH3, p.DMA2_CH2, spi_config);
    info!("SPI setup!");

    let config = RadioConfig {
        power: OutputPower::Db22,
        bandwidth: LoraBandwidth::BW_7,
        spread_factor: LoraSpreadingFactor::SF12,
        ramp_time: RampTime::R3400,
        coding_rate: LoraCodingRate::CR_4_5,
        frequency: 869_075_000,
    };
    let mut radio = Radio::new(spi, cs, busy, reset, dio1, dio2, dio3, dio4, config).await.unwrap();

    info!("Packet Length = {}", radio.read_reg(reg::PACKET_LENGTH).await.unwrap());

    let size = radio.transmit(b"This is a very very long lora message!!!", 10_000.0, true).await.unwrap();

    info!("Transmitted {} bytes!", size);

    loop {
        cortex_m::asm::nop();
    }
}

#[derive(Copy, Clone)]
struct RadioConfig {
    power: OutputPower,
    bandwidth: LoraBandwidth,
    ramp_time: RampTime,
    coding_rate: LoraCodingRate,
    spread_factor: LoraSpreadingFactor,
    frequency: u32,
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            power: OutputPower::Db22,
            bandwidth: LoraBandwidth::BW_500,
            ramp_time: RampTime::R200,
            coding_rate: LoraCodingRate::CR_4_5,
            spread_factor: LoraSpreadingFactor::SF12,
            frequency: 869_525_000
        }
    }
}

struct Radio<'a> {
    spi: Spi<'a, Async, Master>,
    cs: Output<'a>,
    busy: ExtiInput<'a>,
    reset: Output<'a>,
    dio1: ExtiInput<'a>,
    dio2: Input<'a>,
    dio3: Input<'a>,
    dio4: Input<'a>,
    config: RadioConfig,
}

impl<'a> Radio<'a> {
    async fn new(spi: Spi<'a, Async, Master>, cs: Output<'a>, busy: ExtiInput<'a>, reset: Output<'a>, dio1: ExtiInput<'a>, dio2: Input<'a>, dio3: Input<'a>, dio4: Input<'a>, config: RadioConfig) -> Result<Self, spi::Error> {
        let mut radio = Self {
            spi, cs, busy, reset, dio1, dio2, dio3, dio4, config,
        };
        radio.reset().await;
        radio.set_mode(OperatingMode::StbyRc).await?;
        radio.set_regulator_mode(true).await?;
        radio.set_pa_config(config.power).await?;
        // radio.write_op(OpCode::SetDio3AsTcxoCtrl, [TXCOControl::TC_3_3V as u8, 0, 0, 0x64]).await?;
        radio.calibrate_device(CalibrationDevice::ALL).await?;
        radio.calibrate_image(config.frequency).await?;
        radio.write_op(OpCode::SetDIO2AsRfSwitchCtrl, [true as u8]).await?;
        radio.set_packet_type(PacketType::Lora).await?;
        radio.set_rf_freq(config.frequency).await?;
        radio.set_mod_params(config.spread_factor, config.bandwidth, config.coding_rate, false).await?;
        radio.set_buffer_base_address(0, 0).await?;
        radio.set_packet_params(8, LoraHeaderType::VariableLength, 255, true, false).await?;
        radio.set_dio_irq(Irq::ALL, Irq::TX_DONE | Irq::TIMEOUT, Irq::NONE, Irq::NONE).await?;
        radio.set_rx_gain(RxGain::Boosted).await?;
        radio.set_rx_gain_retention().await?;
        radio.tx_clamp_workaround().await?;
        radio.set_tx_params(config.power, config.ramp_time).await?;
        radio.write_op(OpCode::SetRxTxFallbackMode, [FallbackMode::StdbyRc as u8]).await?;
        radio.set_sync_word(LoraNetwork::Private).await?;
        Ok(radio)
    }

    async fn reset(&mut self) {
        self.reset.set_low();
        Timer::after_millis(500).await;
        self.reset.set_high();
    }

    async fn wait_on_busy(&mut self) {
        self.busy.wait_for_low().await;
    }

    async fn read_reg<T>(&mut self, reg: reg::Reg<T>) -> Result<T, spi::Error>
    where
        T: Sized + Default + Copy,
    {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let mut op = RegOp::read(reg.addr(), T::default());
        let payload = unsafe { core::slice::from_raw_parts_mut(&raw mut op as *mut u8, size_of_val(&op)) };
        info!("sent register {:?} read payload: {:02x}", reg, payload);
        self.spi.transfer_in_place(payload).await?;
        Ok(op.payload.payload)
    }

    async fn write_reg<T: Sized>(&mut self, reg: reg::Reg<T>, payload: T) -> Result<(), spi::Error> {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let op = RegOp::write(reg.addr(), payload);
        let payload = unsafe { core::slice::from_raw_parts(&raw const op as *const u8, size_of_val(&op)) };
        info!("sent register {:?} write payload: {:02x}", reg, payload);

        self.spi.write(payload).await?;

        Ok(())
    }

    async fn read_op<T>(&mut self, code: OpCode) -> Result<T, spi::Error>
    where
        T: Sized + Default + Copy,
    {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let mut op = Op::read(code, T::default());
        let payload = unsafe { core::slice::from_raw_parts_mut(&raw mut op as *mut u8, size_of_val(&op)) };

        info!("sent op {:?} read payload: {:02x}", code, payload);

        self.spi.transfer_in_place(payload).await?;
        Ok(op.payload.payload)
    }

    async fn write_op<const N: usize>(&mut self, code: OpCode, payload: [u8; N]) -> Result<(), spi::Error> {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let op = Op { code, payload };
        let payload = unsafe { core::slice::from_raw_parts(&raw const op as *const u8, size_of_val(&op)) };
        info!("written op {:?} payload: {:02x}", code, payload);

        self.spi.write(payload).await?;

        drop(guard);

        self.wait_on_busy().await;

        let (om, cs) = self.get_status().await?;
        info!("op = {}, om = {}, cs = {}", code, om, cs);
        Ok(())
    }

    async fn set_rf_freq(&mut self, freq: u32) -> Result<(), spi::Error> {
        // We convert to u64 to prevent an overflow.
        let rf_freq_raw = ((freq as f32 / FREQ_CONST) as u32).to_be_bytes();

        self.write_op(OpCode::SetRfFrequency, rf_freq_raw).await
    }

    async fn set_mod_params(&mut self, sf: LoraSpreadingFactor, bw: LoraBandwidth, cr: LoraCodingRate, ldr: bool) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetModulationParams, [sf as u8, bw as u8, cr as u8, ldr as u8, 0, 0, 0, 0]).await
    }

    async fn set_buffer_base_address(&mut self, tx_offset: u8, rx_offset: u8) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetBufferBaseAddress, [tx_offset, rx_offset]).await
    }

    async fn set_packet_params(&mut self, preamble_len: u16, header_type: LoraHeaderType, payload_len: u8, crc: bool, inv_iq: bool) -> Result<(), spi::Error> {
        let [preamble_hi, preamble_lo] = preamble_len.to_be_bytes();
        self.write_op(OpCode::SetPacketParams, [preamble_hi, preamble_lo, header_type as u8, payload_len, crc as u8, inv_iq as u8]).await
    }

    async fn set_pa_config(&mut self, power: OutputPower) -> Result<(), spi::Error> {
        let (duty_cycle, hp_max) = match power {
            OutputPower::Db14 => (0x02, 0x02),
            OutputPower::Db17 => (0x02, 0x03),
            OutputPower::Db20 => (0x03, 0x05),
            OutputPower::Db22 => (0x04, 0x07),
        };
        let device = 0;
        let reserved = 1;
        self.write_op(OpCode::SetPAConfig, [duty_cycle, hp_max, device, reserved]).await
    }

    async fn set_tx_params(&mut self, power: OutputPower, ramp_time: RampTime) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetTxParams, [power as u8, ramp_time as u8]).await
    }

    async fn set_tx(&mut self, timeout: f32) -> Result<(), spi::Error> {
        self.clear_irq(Irq::ALL).await?;
        self.mod_quality_workaround(PacketType::Lora, self.config.bandwidth).await?;
        let to_bytes = time_bytes(timeout);
        self.write_op(OpCode::SetTx, to_bytes).await
    }

    async fn set_regulator_mode(&mut self, dc_dc: bool) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetRegulatorMode, [dc_dc as u8]).await
    }

    async fn set_rx_gain(&mut self, gain: RxGain) -> Result<(), spi::Error> {
        self.write_reg(reg::RX_GAIN, gain as u8).await
    }

    /// (6x only) See DS, section 9.6: Receive (RX) Mode).
    async fn set_rx_gain_retention(&mut self) -> Result<(), spi::Error> {
        self.write_reg(reg::RX_GAIN_RETENTION0, 0x01).await?;
        self.write_reg(reg::RX_GAIN_RETENTION1, 0x08).await?;
        self.write_reg(reg::RX_GAIN_RETENTION2, 0xac).await?;
        Ok(())
    }

    /// (6x only) See DS, section 15.2.2.
    async fn tx_clamp_workaround(&mut self) -> Result<(), spi::Error> {
        let cc = self.read_reg::<u8>(reg::TX_CLAMP_CONFIG).await?;
        self.write_reg(reg::TX_CLAMP_CONFIG, cc | 0x1e).await?;
        Ok(())
    }


    /// DS, section 16.1.2. Adapted from pseudocode there.
    /// (6x only)
    async fn mod_quality_workaround(&mut self, packet_type: PacketType, bw: LoraBandwidth) -> Result<(), spi::Error> {
        let mut value = self.read_reg(reg::TX_MODULATION).await?;

        if packet_type == PacketType::Lora && bw == LoraBandwidth::BW_500 {
            value &= 0xFB;
        } else {
            value |= 0x04;
        }

        self.write_reg(reg::TX_MODULATION, value).await
    }

    /// (6x only) See DS, section 15.3.2
    /// "It is advised to add the following commands after ANY Rx with Timeout active sequence, which stop the RTC and clear the
    /// timeout event, if any."
    async fn implicit_header_to_workaround(&mut self) -> Result<(), spi::Error> {
        // todo DS typo: Shows 0920 which is a diff one in code snipped.
        self.write_reg(reg::RTC_CONTROL, 0x00).await?;
        let val = self.read_reg::<u8>(reg::EVENT_MASK).await?;
        self.write_reg(reg::EVENT_MASK, val | 0x02).await
    }

    async fn set_sync_word(&mut self, network: LoraNetwork) -> Result<(), spi::Error> {
        let [sync_word_hi, sync_word_lo] = (network as u16).to_be_bytes();
        self.write_reg(reg::LORA_SYNC_WORD_MSB, sync_word_hi).await?;
        self.write_reg(reg::LORA_SYNC_WORD_LSB, sync_word_lo).await?;
        Ok(())
    }

    async fn clear_irq(&mut self, irqs: Irq) -> Result<(), spi::Error> {
        let [irq_hi, irq_lo] = irqs.bits.to_be_bytes();
        self.write_op(OpCode::ClearIrqStatus, [irq_hi, irq_lo]).await
    }

    async fn get_irq_status(&mut self) -> Result<Irq, spi::Error> {
        let bits = self.read_op(OpCode::GetIrqStatus).await?;
        Ok(Irq::from_bits(bits).unwrap())
    }

    async fn check_status(&mut self) -> Result<(), spi::Error> {
        let (om, cs) = self.get_status().await?;
        info!("        om = {}, cs = {}", om, cs);
        Ok(())
    }

    async fn get_status(&mut self) -> Result<(u8, CommandStatus), spi::Error> {
        let status = self.read_op::<u8>(OpCode::GetStatus).await?;
        let (om, cs) = ((status >> 4) & 0b111, (status >> 1) & 0b111);
        let cs = match cs {
            1 => CommandStatus::CommandProcessSuccess,
            2 => CommandStatus::DataAvailable,
            3 => CommandStatus::CommandTimeout,
            4 => CommandStatus::CommandProcessingError,
            5 => CommandStatus::CommandExecutionError,
            6 => CommandStatus::CommandTxDone,
            other => defmt::panic!("Unknown command status received: {}", other),
        };
        Ok((om, cs))
    }

    async fn set_dio_irq(&mut self, irqs: Irq, dio1: Irq, dio2: Irq, dio3: Irq) -> Result<(), spi::Error> {
        let [irq_hi, irq_lo] = irqs.bits.to_be_bytes();
        let [dio1_hi, dio1_lo] = dio1.bits.to_be_bytes();
        let [dio2_hi, dio2_lo] = dio2.bits.to_be_bytes();
        let [dio3_hi, dio3_lo] = dio3.bits.to_be_bytes();
        self.write_op(OpCode::SetDioIrqParams, [irq_hi, irq_lo, dio1_hi, dio1_lo, dio2_hi, dio2_lo, dio3_hi, dio3_lo]).await
    }

    /// Sets the device into sleep mode; the lowest current consumption possible. Wake up by setting CS low.
    async fn set_mode(&mut self, mode: OperatingMode) -> Result<(), spi::Error> {
        match mode {
            OperatingMode::Sleep(cfg) => {
                // todo: Wake-up on RTC A/R.
                self.write_op(OpCode::SetSleep, [(cfg as u8) << 2]).await
            }
            OperatingMode::StbyRc => self.write_op(OpCode::SetStandby, [0]).await,
            OperatingMode::StbyOsc => self.write_op(OpCode::SetStandby, [1]).await,
            OperatingMode::Fs => self.write_op(OpCode::SetFS, []).await,
            OperatingMode::Tx(timeout) => {
                let to_bytes = time_bytes(timeout);
                self.write_op(OpCode::SetTx, to_bytes).await
            }
            OperatingMode::Rx(timeout) => {
                let to_bytes = time_bytes(timeout);
                self.write_op(OpCode::SetRx, to_bytes).await
            }
        }
    }

    async fn calibrate_device(&mut self, device: CalibrationDevice) -> Result<(), spi::Error> {
        self.write_op(OpCode::Calibrate, [device.bits]).await?;
        Timer::after_millis(5).await; //calibration time for all devices is 3.5mS, SX126x
        Ok(())
    }

    async fn calibrate_image(&mut self, freq: u32) -> Result<(), spi::Error> {
        let payload = if freq > 900000000 {
            [0xE1, 0xE9]
        } else if freq > 850000000 {
            [0xD7, 0xD8]
        } else if freq > 770000000 {
            [0xC1, 0xC5]
        } else if freq > 460000000 {
            [0x75, 0x81]
        } else if freq > 425000000 {
            [0x6B, 0x6F]
        } else {
            [0, 0]
        };
        self.write_op(OpCode::CalibrateImage, payload).await
    }

    async fn set_packet_type(&mut self, packet_type: PacketType) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetPacketType, [packet_type as u8]).await
    }

    pub async fn transmit(&mut self, data: &[u8], timeout: f32, wait: bool) -> Result<u8, spi::Error> {
        self.set_mode(OperatingMode::StbyRc).await?;
        self.set_buffer_base_address(0, 0).await?;
        self.wait_on_busy().await;

        {
            let guard = CsGuard::new(&mut self.cs);
            let mut buffer = [0u8; 255 + 2];
            buffer[0] = OpCode::WriteBuffer as u8;
            buffer[1] = 0;
            buffer[2..(2 + data.len())].copy_from_slice(data);
            self.spi.write::<u8>(&buffer[..(2 + data.len())]).await?;
        }

        let size = data.len() as u8;

        self.write_reg(reg::PAYLOAD_LENGTH, size).await?;

        self.set_tx_params(self.config.power, self.config.ramp_time).await?;

        self.set_dio_irq(Irq::ALL, Irq::TX_DONE | Irq::TIMEOUT, Irq::NONE, Irq::NONE).await?;

        self.set_tx(timeout).await?;

        if wait {
            self.dio1.wait_for_high().await;

            let irq = self.get_irq_status().await?;

            if irq.contains(Irq::TIMEOUT) {
                return Ok(0);
            }
        }

        Ok(size)
    }
}

struct CsGuard<'a, 'b> {
    cs: &'a mut Output<'b>,
}

impl<'a, 'b> CsGuard<'a, 'b> {
    fn new(cs: &'a mut Output<'b>) -> Self {
        cs.set_low();
        Self { cs }
    }
}

impl<'a, 'b> Drop for CsGuard<'a, 'b> {
    fn drop(&mut self) {
        self.cs.set_high();
    }
}

mod reg {
    use core::marker::PhantomData;
    use defmt::Formatter;

    pub const HOPPING_ENABLED: Reg<u8> = Reg::new(0x0385, "Hopping Enabled");
    pub const PACKET_LENGTH: Reg<u8> = Reg::new(0x0386, "Packet Length");
    pub const NB_HOPPING_BLOCKS: Reg<u8> = Reg::new(0x0387, "NB Hopping Blocks");
    pub const PAYLOAD_LENGTH: Reg<u8> = Reg::new(0x0702, "Payload Length");
    /// These sync words must be set to the constants defined at the top of this module.
    pub const LORA_SYNC_WORD_MSB: Reg<u8> = Reg::new(0x0740, "LoRa Sync Word MSB");
    pub const LORA_SYNC_WORD_LSB: Reg<u8> = Reg::new(0x0741, "LoRa Sync Word LSB");
    pub const RNG0: Reg<u8> = Reg::new(0x819, "RNG 0");
    pub const RNG1: Reg<u8> = Reg::new(0x81a, "RNG 1");
    pub const RNG2: Reg<u8> = Reg::new(0x81b, "RNG 2");
    pub const RNG3: Reg<u8> = Reg::new(0x81c, "RNG 3");
    pub const TX_MODULATION: Reg<u8> = Reg::new(0x0889, "TX Modulation");
    pub const RX_GAIN: Reg<u8> = Reg::new(0x08ac, "RX Gain");
    pub const RF_FREQ_31_24: Reg<u8> = Reg::new(0x088B, "RF Freq 31-24");
    pub const RF_FREQ_23_16: Reg<u8> = Reg::new(0x088C, "RF Freq 23-16");
    pub const RF_FREQ_15_8: Reg<u8> = Reg::new(0x088D, "RF Freq 15-8");
    pub const RF_FREQ_7_0: Reg<u8> = Reg::new(0x088E, "RF Freq 7-0");
    pub const TX_CLAMP_CONFIG: Reg<u8> = Reg::new(0x08d8, "TX Clamp Config");
    pub const OCP_CONFIG: Reg<u8> = Reg::new(0x08e7, "OCP Config");
    pub const RTC_CONTROL: Reg<u8> = Reg::new(0x0902, "RTC Control");
    pub const XTA_TRIM: Reg<u8> = Reg::new(0x0911, "XT A Trim");
    pub const XTB_TRIM: Reg<u8> = Reg::new(0x0912, "XT B Trim");
    pub const DIO3_OUTPUT_V_CONTROL: Reg<u8> = Reg::new(0x0920, "DIO3 Output V Control");
    pub const EVENT_MASK: Reg<u8> = Reg::new(0x0944, "Event Mask");
    /// These three registers aren't listed in Table 12.1, but apparently
    /// exist from the DS-included RxGain retention workaround.
    pub const RX_GAIN_RETENTION0: Reg<u8> = Reg::new(0x029f, "RX Gain Retention 0");
    pub const RX_GAIN_RETENTION1: Reg<u8> = Reg::new(0x02a0, "RX Gain Retention 1");
    pub const RX_GAIN_RETENTION2: Reg<u8> = Reg::new(0x02a1, "RX Gain Retention 2");

    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct Reg<T>(u16, &'static str, PhantomData<T>);

    impl<T> Reg<T> {
        const fn new(addr: u16, name: &'static str) -> Self {
            Self(addr, name, PhantomData)
        }

        pub fn addr(&self) -> u16 {
            self.0
        }
    }

    impl<T> defmt::Format for Reg<T> {
        fn format(&self, fmt: Formatter) {
            self.1.format(fmt)
        }
    }
}
#[derive(Clone, Copy, defmt::Format)]
#[allow(dead_code)]
#[repr(u16)]
/// Registers, to read and write following the appropriate OpCode.
/// See DS, section 12.1: Register Table
pub enum Register {
    HoppingEnabled = 0x0385,
    PacketLength = 0x0386, // FSK
    NbHoppingBLocks = 0x0387,
    // todo: Sort out how these nbsymbols and freqs work.
    NbSymbols0 = 0x0388,
    NbSymbols0b = 0x0389,
    Freq0a = 0x038a,
    Freq0b = 0x038b,
    Freq0c = 0x038c,
    Freq0d = 0x038d,
    DioxOutputEnable = 0x0580,
    DioxInputEnable = 0x0583,
    DioxPullUpControl = 0x0584,
    DioxPullDownControl = 0x0585,
    WhiteningInitialValueMsb = 0x06b8,
    WhiteningInitialValueLsb = 0x06b9,
    CrcMsbInitialValue0 = 0x06bc,
    CrcMsbInitialValue1 = 0x06bd,
    CrcMsbPolynomialValue0 = 0x06be,
    CrcLsbPolynomialValue1 = 0x06bf,
    /// These are for FSK. Bytes of the sync word.
    SyncWord0 = 0x06c0,
    SyncWord1 = 0x06c1,
    SyncWord2 = 0x06c2,
    SyncWord3 = 0x06c3,
    SyncWord4 = 0x06c4,
    SyncWord5 = 0x06c5,
    SyncWord6 = 0x06c6,
    SyncWord7 = 0x06c7,
    NodeAddress = 0x06cd,
    BroadcastAddress = 0x06ce,
    PayloadLength = 0x0702,
    IqPolaritySetup = 0x0736,
    /// These sync words must be set to the constants defined at the top of this module.
    LoraSyncWordMsb = 0x0740,
    LoraSyncWordLsb = 0x0741,
    RandomNumGen0 = 0x819,
    RandomNumGen1 = 0x81a,
    RandomNumGen2 = 0x81b,
    RandomNumGen3 = 0x81c,
    TxModulation = 0x0889,
    RxGain = 0x08ac,
    TxClampConfig = 0x08d8,
    OcpConfiguration = 0x08e7,
    RtcControl = 0x0902,
    XtaTrim = 0x0911,
    XtbTrim = 0x912,
    Dio3OutputVoltageControl = 0x0920,
    EventMask = 0x0944,
    /// These three registers aren't listed in Table 12.1, but apparently
    /// exist from the DS-included RxGain retention workaround.
    RxGainRetention0 = 0x029f,
    RxGainRetention1 = 0x02a0,
    RxGainRetention2 = 0x02a1,
}

/// 6x: DS, section 13.1.1. Table 13-2. For bit 2.
/// 8x: DS, section 11.6.1. Table 11-17. For bit 0.
#[repr(u8)]
#[derive(Clone, Copy, Format)]
pub enum SleepConfig {
    /// "Ram flushed" on 8x.
    ColdStart = 0,
    /// "Ram retained" on 8x.
    WarmStart = 1,
}

/// 6x DS, section 9. (And table 13-76) 8x: Table 11-5. (Called Circuit mode)
#[derive(Clone, Copy, Format)]
#[allow(dead_code)]
pub enum OperatingMode {
    /// In this mode, most of the radio internal blocks are powered down or in low power mode and optionally the RC64k clock
    /// and the timer are running.
    Sleep(SleepConfig),
    /// In standby mode the host should configure the chip before going to RX or TX modes. By default in this state, the system is
    /// clocked by the 13 MHz RC oscillator to reduce power consumption (in all other modes except SLEEP the XTAL is turned ON).
    /// However, if the application is time-critical, the XOSC block can be turned or left ON.
    StbyRc,
    StbyOsc,
    /// In FS mode, PLL and related regulators are switched ON. The BUSY goes low as soon as the PLL is locked or timed out.
    /// The command SetFs() is used to set the device in the frequency synthesis mode where the PLL is locked to the carrier
    /// frequency. This mode is used for test purposes of the PLL and can be considered as an intermediate mode. It is
    /// automatically reached when going from STDBY_RC mode to TX mode or RX mode.
    Fs,
    /// The inner value is the timeout, in ms.
    Tx(f32),
    Rx(f32),
}

#[derive(Clone, Copy, PartialEq, defmt::Format)]
#[allow(dead_code)]
#[repr(u8)]
/// Sx126x: See Table 11-1.
/// todo: I think this table was lifted from a rust lib, and is a mix of 126x and 128x commands.
/// todo: For now, use for sx126x, and override these for sx128x below in the method.
pub enum OpCode {
    GetStatus = 0xC0,
    WriteRegister = 0x0D,
    ReadRegister = 0x1D,
    WriteBuffer = 0x0E,
    ReadBuffer = 0x1E,
    SetSleep = 0x84,
    SetStandby = 0x80,
    SetFS = 0xC1,
    SetTx = 0x83,
    SetRx = 0x82,
    SetRxDutyCycle = 0x94,
    SetCAD = 0xC5,
    SetTxContinuousWave = 0xD1,
    SetTxContinuousPremable = 0xD2,
    SetPacketType = 0x8A,
    GetPacketType = 0x11,
    SetRfFrequency = 0x86,
    SetTxParams = 0x8E,
    SetPAConfig = 0x95,
    SetCADParams = 0x88,
    SetBufferBaseAddress = 0x8F,
    SetModulationParams = 0x8B,
    SetPacketParams = 0x8C,
    GetRxBufferStatus = 0x13,
    GetPacketStatus = 0x14,
    GetRSSIInst = 0x15,
    GetStatistics = 0x10,
    ResetStats = 0x00,
    SetDioIrqParams = 0x08,
    GetIrqStatus = 0x12,
    ClearIrqStatus = 0x02,
    Calibrate = 0x89,
    CalibrateImage = 0x98,
    SetRegulatorMode = 0x96,
    GetDeviceErrors = 0x17,
    ClrErrors = 0x07,
    SetDio3AsTcxoCtrl = 0x97,
    SetRxTxFallbackMode = 0x93,
    SetDIO2AsRfSwitchCtrl = 0x9d,
    SetStopRxTimerOnPreamble = 0x9F,
    SetLoRaSymbTimeout = 0xA0,
    // below: sx128x only. Ommitted for now due to conflicts.
    SetSaveContext = 0xd5,
    // SetAutoTx = 0x98,
    SetLongPreamble = 0x9b,
    // SetUartSpeed = 0x9d,
    SetRangingRole = 0xa3,
    SetAdvancedRnaging = 0x91,
}

/// 6x DS, 13.4.2. Table 13-38.  The switch from one frame to another must be done in STDBY_RC mode.
/// 8x: Table 11-42.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format)]
#[allow(dead_code)]
pub enum PacketType {
    /// (G)Fsk
    Gfsk = 0,
    Lora = 1,
    None = 15
}

#[repr(u16)]
#[derive(Clone, Copy, Format)]
#[allow(dead_code)]
/// (SX126x only(?) DS, table 12-1. Differentiate the LoRa signal for Public or Private network.
/// set the `LoRa Sync word MSB and LSB values to this.
pub enum LoraNetwork {
    Public = 0x3444,  // corresponds to sx127x 0x34
    Private = 0x1424, // corresponds to sx127x 0x12
}

/// DS, Table 13-41. Power ramp time. Titles correspond to ramp time in µs.
/// todo: Figure out guidelines for setting this. The DS doesn't have much on it.
#[repr(u8)]
#[derive(Clone, Copy, Format)]
#[allow(dead_code)]
pub enum RampTime {
    R10 = 0,
    R20 = 1,
    R40 = 2,
    R80 = 3,
    R200 = 4,
    R800 = 5,
    R1700 = 6,
    R3400 = 7,
}

bitflags! {
    pub struct Irq: u16 {
        const NONE = 0;
        const TX_DONE = 0x1;
        const RX_DONE = 0x2;
        const PREAMBLE_DETECTED = 0x4;
        const SYNCWORD_VALID = 0x8;
        const HEADER_VALID = 0x10;
        const HEADER_ERROR = 0x20;
        const CRC_ERROR = 0x40;
        const CAD_DONE = 0x80;
        const CAD_ACTIVITY_DETECTED = 0x100;
        const TIMEOUT = 0x200;
        const ALL = 0xFFFF;
    }
}

// Oscillator frequency in Mhz.
const F_XTAL: f32 = 32_000_000.;
// These constants are pre-computed
const FREQ_CONST: f32 = F_XTAL / (1 << 25) as f32;

// These constants are pre-computed
const TIMING_FACTOR_MS: f32 = 0.015_625;

/// Convert a f32 time in ms to 3 24-bit unsigned integer bytes, used with the radio's system. Used for
/// sleep, and Rx duration.
/// This is defined a few times in the datasheet, including section 13.1.4.
pub fn time_bytes(time_ms: f32) -> [u8; 3] {
    // Sleep Duration = sleepPeriod * 15.625 µs
    let result = ((time_ms / TIMING_FACTOR_MS) as u32).to_be_bytes();
    [result[1], result[2], result[3]]
}

#[repr(packed)]
struct Op<P> {
    code: OpCode,
    payload: P,
}

impl<T> Op<ReadPayload<T>> {
    fn read(code: OpCode, payload: T) -> Self {
        Self {
            code,
            payload: ReadPayload {
                status: 0,
                payload,
            },
        }
    }
}

impl<T> Op<WritePayload<T>> {
    fn write(code: OpCode, payload: T) -> Self {
        Self {
            code,
            payload: WritePayload {
                payload,
            },
        }
    }
}

#[repr(packed)]
struct RegOp<P> {
    code: OpCode,
    addr_hi: u8,
    addr_lo: u8,
    payload: P,
}

#[repr(packed)]
struct ReadPayload<T> {
    status: u8,
    payload: T,
}

#[repr(packed)]
struct WritePayload<T> {
    payload: T,
}

impl<T> RegOp<ReadPayload<T>> {
    fn read(addr: u16, payload: T) -> Self {
        let [addr_hi, addr_lo] = addr.to_be_bytes();
        Self {
            code: OpCode::ReadRegister,
            addr_hi,
            addr_lo,
            payload: ReadPayload {
                status: 0,
                payload,
            },
        }
    }
}

impl<T> RegOp<WritePayload<T>> {
    fn write(addr: u16, payload: T) -> Self {
        let [addr_hi, addr_lo] = addr.to_be_bytes();
        Self {
            code: OpCode::WriteRegister,
            addr_hi,
            addr_lo,
            payload: WritePayload {
                payload,
            },
        }
    }
}


/// (SX126x) DS, Table 13-47. Mod param 1.
/// (SX128x) DS, Table 14-47. Mod param 1.
/// "A higher spreading factor provides better receiver sensitivity at the expense of longer
/// transmission times (time-on-air)."
#[repr(u8)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum LoraSpreadingFactor {
    SF5 = 0x05,
    SF6 = 0x06,
    SF7 = 0x07,
    SF8 = 0x08,
    SF9 = 0x09,
    SF10 = 0x0A,
    SF11 = 0x0B,
    SF12 = 0x0C,
}

/// DS, Table 13-47. Mod param 2.
/// "An increase in signal bandwidth permits the use of a higher effective data rate, thus reducing transmission time at the
/// expense of reduced sensitivity improvement."
/// Note that the lower settings here can result in 5s or higher OTA time! OTA seems to scale linearly (inversely)
/// with bandwidth.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
#[allow(non_camel_case_types, dead_code)]
pub enum LoraBandwidth {
    BW_7 = 0x00,
    BW_10 = 0x08,
    BW_15 = 0x01,
    BW_20 = 0x09,
    BW_31 = 0x02,
    BW_41 = 0x0A,
    BW_62 = 0x03,
    BW_125 = 0x04,
    /// May not be available below 400Mhz)
    BW_250 = 0x05,
    /// May not be available below 400Mhz)
    BW_500 = 0x06,
}

/// SX126x: DS, Table 13-49. Mod param 3.
/// SX128x: DS, Table 14-49. Mod param 3.
/// "A higher coding rate provides better noise immunity at the expense of longer transmission time. In normal conditions a
/// factor of 4/5 provides the best trade-off; in the presence of strong interfererence a higher coding rate may be used. Error
/// correction code does not have to be known in advance by the receiver since it is encoded in the header part of the packet."
#[repr(u8)]
#[derive(Clone, Copy)]
#[allow(non_camel_case_types, dead_code)]
pub enum LoraCodingRate {
    /// raw/total bits: 4/5. Overhead ratio: 1.25
    CR_4_5 = 1,
    /// raw/total bits: 4/6. Overhead ratio: 1.5
    CR_4_6 = 2,
    /// raw/total bits: 4/7. Overhead ratio: 1.75
    CR_4_7 = 3,
    /// raw/total bits: 4/8. Overhead ratio: 2.0
    CR_4_8 = 4,
    /// These CR_LIs are sx128x only:
    /// "* A new interleaving scheme has been implemented to increase robustness to burst interference and/or strong Doppler
    /// events. The FEC has been kept the same to limit the impact on complexity."
    CR_LI_4_5 = 5,
    CR_LI_4_6 = 6,
    CR_LI_4_8 = 7,
}

#[repr(u8)]
#[derive(Clone, Copy)]
#[allow(dead_code)]
/// (SX126x only) Table 13-50. Mod param 4.
/// "For low data rates (typically for high SF or low BW) and very long payloads which may last several seconds in the air, the low
/// data rate optimization (LDRO) can be enabled. This reduces the number of bits per symbol to the given SF minus two (see
/// Section 6.1.4 "LoRa® Time-on-Air" on page 41) in order to allow the receiver to have a better tracking of the LoRa® signal.
/// Depending on the payload size, the low data rate optimization is usually recommended when a LoRa® symbol time is equal
/// or above 16.38 ms."
pub enum LoraLdrOptimization {
    Disabled = 0,
    Enabled = 1,
}

/// SX126x DS, Table 13-67. Packet param 3.
/// SX128x DS, Table 14-51. Packet param 2.
/// Also, Section 6.1.3. "The LoRa® modem employs two types of packet formats: explicit and implicit. The explicit
/// packet includes a short header
/// that contains information about the number of bytes, coding rate and whether a CRC is used in the packet."

#[repr(u8)]
#[derive(Clone, Copy)]
pub enum LoraHeaderType {
    /// Explict header
    VariableLength = 0x00,
    /// Implicit header
    FixedLength = 0x01,
}

/// Table 13-21
/// These don't take into account the external PA, if applicable.
/// Note that the values are listed for sx1262. They are hard-coded for high power PA selection.
#[repr(u8)] // For storing in configs.
#[derive(Clone, Copy)]
pub enum OutputPower {
    /// 25mW
    Db14 = 0x0e,
    /// 50mW
    Db17 = 0x11,
    /// 100mW
    Db20 = 0x14,
    /// 158mW
    Db22 = 0x16,
}

/// 6x only. DS, 13.1.15. This defines the mode the radio goes into after a successful Tx or Rx.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum FallbackMode {
    Fs = 0x40,
    StdbyXosc = 0x30,
    StdbyRc = 0x20,
}

// todo: 6x only? Can't tell
/// 6x: DS, section 9.6: Receive (RX) Mode
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum RxMode {
    Continuous,
    Single,
    SingleWithTimeout,
    Listen,
}


/// (6x): DS, section 13.5.1. 8x: Table 11-5
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
pub enum CommandStatus {
    /// Transceiver has successfully processed the command.
    /// 8x only.
    CommandProcessSuccess = 1,
    /// A packet has been successfully received and data can be retrieved
    DataAvailable = 2,
    /// A transaction from host took too long to complete and triggered an internal watchdog. The watchdog mechanism can be disabled by host; it
    /// is meant to ensure all outcomes are flagged to the host MCU
    CommandTimeout = 3,
    /// Processor was unable to process command either because of an invalid opcode or because an incorrect number of parameters has been
    /// provided.
    CommandProcessingError = 4,
    /// The command was successfully processed, however the chip could not execute the command; for instance it was unable to enter the specified
    /// device mode or send the requested data
    CommandExecutionError = 5,
    /// The transmission of the current packet has terminated
    CommandTxDone = 6,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
#[allow(non_camel_case_types, dead_code)]
pub enum TXCOControl {
    TC_1_6V = 0x00,
    TC_1_7V = 0x01,
    TC_1_8V = 0x02,
    TC_2_2V = 0x03,
    TC_2_4V = 0x04,
    TC_2_7V = 0x05,
    TC_3_0V = 0x06,
    TC_3_3V = 0x07,
}

bitflags! {
    pub struct CalibrationDevice: u8 {
        const RC64K_CLK  = 0x01;
        const RC13M_CLK  = 0x02;
        const PLL        = 0x04;
        const ADC_PULSE  = 0x08;
        const ADC_BULK_N = 0x10;
        const ADC_BULK_P = 0x20;
        const IMAGE      = 0x40;
        const ALL        = 0x7F;
    }
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
#[allow(non_camel_case_types, dead_code)]
pub enum RxGain {
    PowerSave = 0x94,
    Boosted   = 0x96,
}