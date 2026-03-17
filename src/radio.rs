use defmt::{bitflags, error, info, warn, Format};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Input, Output};
use embassy_stm32::mode::Async;
use embassy_stm32::spi;
use embassy_stm32::spi::mode::Master;
use embassy_stm32::spi::Spi;
use embassy_time::Timer;

#[derive(Copy, Clone)]
pub struct RadioConfig {
    pub power: OutputPower,
    pub bandwidth: LoraBandwidth,
    pub ramp_time: RampTime,
    pub coding_rate: LoraCodingRate,
    pub spread_factor: LoraSpreadingFactor,
    pub header_type: LoraHeaderType,
    pub preamble_len: u16,
    pub frequency: u32,
    pub sync_word: u16
}

impl Default for RadioConfig {
    fn default() -> Self {
        Self {
            power: OutputPower::Db22,
            bandwidth: LoraBandwidth::BW_500,
            ramp_time: RampTime::R200,
            coding_rate: LoraCodingRate::CR_4_5,
            spread_factor: LoraSpreadingFactor::SF12,
            header_type: LoraHeaderType::VariableLength,
            sync_word: 0x3444, //LoRa Public
            preamble_len: 8,
            frequency: 869_525_000
        }
    }
}

pub struct Radio<'a> {
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
    pub async fn new(spi: Spi<'a, Async, Master>, cs: Output<'a>, busy: ExtiInput<'a>, reset: Output<'a>, dio1: ExtiInput<'a>, dio2: Input<'a>, dio3: Input<'a>, dio4: Input<'a>, config: RadioConfig) -> Result<Self, spi::Error> {
        let mut radio = Self {
            spi, cs, busy, reset, dio1, dio2, dio3, dio4, config,
        };
        let mut buffer = [0; 16];
        radio.reset().await;
        radio.wait_on_busy().await;
        Timer::after_secs(1).await;
        radio.set_standby(Standby::Rc).await?;
        warn!("Module: {}", radio.get_version_string(&mut buffer).await?);
        radio.set_buffer_base_address(0, 0).await?;
        radio.set_packet_type(PacketType::Lora).await?;
        radio.write_op(OpCode::SetRxTxFallbackMode, [FallbackMode::StdbyRc as u8]).await?;
        radio.set_cad_params(CadSymbol::Symbol8, 22, 12, CadExitMode::CadOnly, 0.0).await?;
        radio.clear_irq(Irq::ALL).await?;
        radio.set_dio_irq(Irq::NONE, Irq::NONE, Irq::NONE, Irq::NONE).await?;
        radio.calibrate_device(CalibrationDevice::ALL).await?;
        radio.set_regulator_mode(true).await?;
        let _pt = radio.get_packet_type().await?;
        radio.set_mod_params(config.spread_factor, config.bandwidth, config.coding_rate, false).await?;
        let _pt = radio.get_packet_type().await?;
        radio.set_sync_word(config.sync_word).await?;
        let _pt = radio.get_packet_type().await?;
        let iq_pol = radio.read_reg(reg::IQ_POLARITY_SETUP).await?;
        radio.write_reg(reg::IQ_POLARITY_SETUP, iq_pol & !(1 << 2)).await?;
        radio.set_packet_params(config.preamble_len, config.header_type, 255, true, false).await?;
        radio.set_ocp_config(0x38).await?;
        radio.write_op(OpCode::SetDIO2AsRfSwitchCtrl, [true as u8]).await?;
        let _pt = radio.get_packet_type().await?;
        let iq_pol = radio.read_reg(reg::IQ_POLARITY_SETUP).await?;
        radio.write_reg(reg::IQ_POLARITY_SETUP, iq_pol & !(1 << 2)).await?;
        radio.set_packet_params(config.preamble_len, config.header_type, 255, true, false).await?;
        let _pt = radio.get_packet_type().await?;
        radio.set_packet_params(config.preamble_len, config.header_type, 255, true, false).await?;
        let _pt = radio.get_packet_type().await?;
        radio.set_mod_params(config.spread_factor, LoraBandwidth::BW_500, config.coding_rate, false).await?;
        let _pt = radio.get_packet_type().await?;
        radio.set_mod_params(config.spread_factor, config.bandwidth, config.coding_rate, false).await?;
        // radio.set_tcxo_mode(TcxoCtrlVoltage::TC_1_7V, 100).await?;
        radio.calibrate_image(config.frequency).await?;
        radio.set_rf_freq(config.frequency).await?;
        radio.tx_clamp_workaround().await?;
        let ocp = radio.get_ocp_config().await?;
        radio.set_pa_config(config.power).await?;
        radio.set_tx_params(config.power, config.ramp_time).await?;
        radio.set_ocp_config(ocp).await?;
        radio.set_ocp_config(0x38).await?;
        radio.write_op(OpCode::SetDIO2AsRfSwitchCtrl, [true as u8]).await?;
        radio.set_rx_gain(RxGain::Boosted).await?;
        radio.set_rx_gain_retention().await?;
        let u = radio.read_reg(reg::UNK_08_B5).await?;
        radio.write_reg(reg::UNK_08_B5, 0x05).await?;
        let u = radio.read_reg(reg::UNK_08_B5).await?;
        let _pt = radio.get_packet_type().await?;
        let iq_pol = radio.read_reg(reg::IQ_POLARITY_SETUP).await?;
        radio.write_reg(reg::IQ_POLARITY_SETUP, iq_pol & !(1 << 2)).await?;
        radio.set_packet_params(config.preamble_len, config.header_type, 255, true, false).await?;
        radio.set_dio_irq(Irq::ALL, Irq::ALL, Irq::NONE, Irq::NONE).await?;
        Ok(radio)
    }

    pub async fn reset(&mut self) {
        Timer::after_millis(10).await;
        self.reset.set_low();
        Timer::after_millis(20).await;
        self.reset.set_high();
        Timer::after_millis(10).await;
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

        self.spi.transfer_in_place(payload).await?;
        Ok(op.payload.payload)
    }

    async fn write_reg<T: Sized>(&mut self, reg: reg::Reg<T>, payload: T) -> Result<(), spi::Error> {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let op = RegOp::write(reg.addr(), payload);
        let payload = unsafe { core::slice::from_raw_parts(&raw const op as *const u8, size_of_val(&op)) };

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

        self.spi.transfer_in_place(payload).await?;
        Ok(op.payload.payload)
    }

    async fn write_op<const N: usize>(&mut self, code: OpCode, payload: [u8; N]) -> Result<(), spi::Error> {
        self.wait_on_busy().await;

        let guard = CsGuard::new(&mut self.cs);

        let op = Op { code, payload };
        let payload = unsafe { core::slice::from_raw_parts(&raw const op as *const u8, size_of_val(&op)) };

        self.spi.write(payload).await?;
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

    async fn set_cad_params(&mut self, cad_symbol: CadSymbol, peak_threshold: u8, min_threshold: u8, exit_mode: CadExitMode, timeout: f32) -> Result<(), spi::Error> {
        let to_bytes = time_bytes(timeout);
        self.write_op(OpCode::SetCADParams, [cad_symbol as u8, peak_threshold, min_threshold, exit_mode as u8, to_bytes[0], to_bytes[1], to_bytes[2]]).await
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

    async fn set_tcxo_mode(&mut self, voltage: TcxoCtrlVoltage, timeout: u32) -> Result<(), spi::Error> {
        let to_bytes = timeout.to_be_bytes();
        warn!("voltage bytes: {:02x}", [to_bytes[1], to_bytes[2], to_bytes[3]]);
        self.write_op(OpCode::SetDio3AsTcxoCtrl, [voltage as u8, to_bytes[1], to_bytes[2], to_bytes[3]]).await
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

    async fn set_rx(&mut self, timeout: Option<f32>) -> Result<(), spi::Error> {
        self.clear_irq(Irq::ALL).await?;
        if let Some(timeout) = timeout {
            let to_bytes = time_bytes(timeout);
            self.write_op(OpCode::SetRx, to_bytes).await
        } else {
            self.write_op(OpCode::SetRx, [0xFF, 0xFF, 0xFF]).await
        }
    }

    async fn set_regulator_mode(&mut self, dc_dc: bool) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetRegulatorMode, [dc_dc as u8]).await
    }

    async fn set_rx_gain(&mut self, gain: RxGain) -> Result<(), spi::Error> {
        self.write_reg(reg::RX_GAIN, gain as u8).await
    }

    /// (6x only) See DS, section 9.6: Receive (RX) Mode).
    async fn set_rx_gain_retention(&mut self) -> Result<(), spi::Error> {
        self.write_reg(reg::RX_GAIN_RETENTION, [0x01, 0x08, 0xac]).await
    }

    //TODO
    async fn get_ocp_config(&mut self) -> Result<u8, spi::Error> {
        self.read_reg(reg::OCP_CONFIG).await
    }

    //TODO
    async fn set_ocp_config(&mut self, ocp: u8) -> Result<(), spi::Error> {
        self.write_reg(reg::OCP_CONFIG, ocp).await
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

    async fn set_sync_word(&mut self, word: u16) -> Result<(), spi::Error> {
        let [word_hi, word_lo] = word.to_be_bytes();
        self.write_reg(reg::LORA_SYNC_WORD, [word_hi, word_lo]).await?;
        Ok(())
    }

    async fn clear_irq(&mut self, irqs: Irq) -> Result<(), spi::Error> {
        let [irq_hi, irq_lo] = irqs.bits.to_be_bytes();
        self.write_op(OpCode::ClearIrqStatus, [irq_hi, irq_lo]).await
    }

    async fn wait_for_irq(&mut self) -> Result<Irq, spi::Error> {
        self.dio1.wait_for_high().await;
        self.get_irq_status().await
    }

    async fn get_irq_status(&mut self) -> Result<Irq, spi::Error> {
        let bits = self.read_op(OpCode::GetIrqStatus).await?;
        Ok(Irq::from_bits(u16::from_be(bits)).unwrap())
    }

    async fn get_status(&mut self) -> Result<(OperatingMode, CommandStatus), spi::Error> {
        let status = self.read_op::<u8>(OpCode::GetStatus).await?;
        let (om, cs) = ((status >> 4) & 0b111, (status >> 1) & 0b111);
        let om = match om {
            0 => OperatingMode::StbyRc,
            1 => OperatingMode::StbyOsc,
            2 => OperatingMode::Fs,
            3 => OperatingMode::Tx,
            4 => OperatingMode::Rx,
            5 => OperatingMode::RxDc,
            6 => OperatingMode::Cad,
            7 => OperatingMode::Sleep,
            other => defmt::panic!("Unknown operating mode received: {}", other),
        };
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

    async fn set_standby(&mut self, standby: Standby) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetStandby, [standby as u8]).await
    }

    async fn set_sleep(&mut self, cfg: SleepConfig) -> Result<(), spi::Error> {
        self.write_op(OpCode::SetSleep, [(cfg as u8) << 2]).await
    }

    async fn calibrate_device(&mut self, device: CalibrationDevice) -> Result<(), spi::Error> {
        self.write_op(OpCode::Calibrate, [device.bits]).await?;
        Timer::after_millis(5).await; //calibration time for all devices is 3.5mS, SX126x
        self.wait_on_busy().await;
        Ok(())
    }

    async fn calibrate_image(&mut self, freq: u32) -> Result<(), spi::Error> {
        let payload = if freq > 900000000 {
            [0xE1, 0xE9]
        } else if freq > 850000000 {
            [0xD7, 0xDB]
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

    async fn get_packet_type(&mut self) -> Result<PacketType, spi::Error> {
        Ok(match self.read_op(OpCode::GetPacketType).await? {
            0 => PacketType::Gfsk,
            1 => PacketType::Lora,
            _ => PacketType::None,
        })
    }

    async fn get_version_string<'b>(&mut self, buffer: &'b mut [u8; 16]) -> Result<&'b str, spi::Error> {
        let s = self.read_reg(reg::VERSION_STRING).await?;
        let len = s.iter().position(|c| *c == 0).unwrap_or(16);
        *buffer = s;
        Ok(unsafe { core::str::from_utf8_unchecked(&buffer[..len]) } )
    }

    pub async fn transmit(&mut self, data: &[u8], timeout: f32, wait: bool) -> Result<u8, spi::Error> {
        self.set_standby(Standby::Rc).await?;
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
            let irq = self.wait_for_irq().await?;

            if irq.contains(Irq::TIMEOUT) {
                return Ok(0);
            }
        }

        Ok(size)
    }

    pub async fn receive(&mut self, buffer: &mut [u8; 255], timeout: Option<f32>, wait: bool) -> Result<usize, spi::Error> {
        self.set_dio_irq(Irq::ALL, Irq::RX_DONE, Irq::NONE, Irq::NONE).await?;
        self.set_rx(timeout).await?;

        if wait {
            let irq = self.wait_for_irq().await?;
            self.clear_irq(irq).await?;

            /*if irq.contains(Irq::PREAMBLE_DETECTED) {
                info!("{}", irq);
                return Ok(0);
            }*/

            if irq.contains(Irq::HEADER_ERROR) {
                error!("{}", irq);
                self.set_standby(Standby::Rc).await?;
                return Ok(0);
            }

            let (om, cs) = self.get_status().await?;

            if cs == CommandStatus::CommandTimeout {
                return Ok(0);
            }

            // info!("RX om = {}, cs = {}", om, cs);

            if cs == CommandStatus::DataAvailable {
                if irq.contains(Irq::HEADER_ERROR) || irq.contains(Irq::CRC_ERROR) {
                    error!("HEADER/CRC error!");
                    return Ok(0);
                }
            }

            let (size, offset) = self.read_op::<(u8, u8)>(OpCode::GetRxBufferStatus).await?;
            let size = if self.config.header_type == LoraHeaderType::FixedLength {
                self.read_reg(reg::PAYLOAD_LENGTH).await? as usize
            } else {
                size as usize
            };
            defmt::trace!("size = {}, offset = {}", size, offset);

            self.set_standby(Standby::Rc).await?;
            self.implicit_header_to_workaround().await?;

            let device_errors = self.read_op::<u16>(OpCode::GetDeviceErrors).await?;
            if device_errors != 0 {
                error!("device error: {}", device_errors);
            }

            // info!("IRQ: {}", irq);

            if cs == CommandStatus::DataAvailable {
                let (rssi, snr, sig_rssi) = self.read_op::<(u8, u8, u8)>(OpCode::GetPacketStatus).await?;
                let rssi = (-(rssi as i8)) >> 1;
                let snr = (snr as i8 + 2) >> 2;
                let sig_rssi = (-(sig_rssi as i8)) >> 1;

                let guard = CsGuard::new(&mut self.cs);

                // info!("rssi = {}, snr = {}, sig_rssi = {}", rssi, snr, sig_rssi);
                self.spi.transfer_in_place(&mut [OpCode::ReadBuffer as u8, 0, 0]).await?;
                self.spi.read(buffer).await?;
                return Ok(size);
            }
        }

        Ok(0)
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

    pub const VERSION_STRING: Reg<[u8; 16]> = Reg::new(0x0320, "Version String");
    pub const HOPPING_ENABLED: Reg<u8> = Reg::new(0x0385, "Hopping Enabled");
    pub const PACKET_LENGTH: Reg<u8> = Reg::new(0x0386, "Packet Length");
    pub const NB_HOPPING_BLOCKS: Reg<u8> = Reg::new(0x0387, "NB Hopping Blocks");
    pub const PAYLOAD_LENGTH: Reg<u8> = Reg::new(0x0702, "Payload Length");
    /// These sync words must be set to the constants defined at the top of this module.
    pub const LORA_SYNC_WORD: Reg<[u8; 2]> = Reg::new(0x0740, "LoRa Sync Word");
    pub const IQ_POLARITY_SETUP: Reg<u8> = Reg::new(0x0736, "IQ Polarity Setup");
    pub const UNK_08_B5: Reg<u8> = Reg::new(0x08B5, "0x08B5");
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
    pub const OCP_CONFIG: Reg<u8> = Reg::new(0x08e7, "OverCurrentProtection Config");
    pub const RTC_CONTROL: Reg<u8> = Reg::new(0x0902, "RTC Control");
    pub const XTA_TRIM: Reg<u8> = Reg::new(0x0911, "XT A Trim");
    pub const XTB_TRIM: Reg<u8> = Reg::new(0x0912, "XT B Trim");
    pub const DIO3_OUTPUT_V_CONTROL: Reg<u8> = Reg::new(0x0920, "DIO3 Output V Control");
    pub const EVENT_MASK: Reg<u8> = Reg::new(0x0944, "Event Mask");
    /// These three registers aren't listed in Table 12.1, but apparently
    /// exist from the DS-included RxGain retention workaround.
    pub const RX_GAIN_RETENTION: Reg<[u8; 3]> = Reg::new(0x029f, "RX Gain Retention");

    #[derive(Copy, Clone, PartialEq, Eq)]
    pub struct Reg<T>(u16, &'static str, PhantomData<T>);

    impl<T> Reg<T> {
        const fn new(addr: u16, name: &'static str) -> Self {
            Self(addr, name, PhantomData)
        }

        pub const fn add(&self, offset: usize) -> Reg<T> {
            Reg::new(self.0 + offset as u16, self.1)
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
#[repr(u8)]
#[derive(Clone, Copy, Format)]
#[allow(dead_code)]
pub enum OperatingMode {
    /// In this mode, most of the radio internal blocks are powered down or in low power mode and optionally the RC64k clock
    /// and the timer are running.
    Sleep = 7,
    /// In standby mode the host should configure the chip before going to RX or TX modes. By default in this state, the system is
    /// clocked by the 13 MHz RC oscillator to reduce power consumption (in all other modes except SLEEP the XTAL is turned ON).
    /// However, if the application is time-critical, the XOSC block can be turned or left ON.
    StbyRc = 0,
    StbyOsc = 1,
    /// In FS mode, PLL and related regulators are switched ON. The BUSY goes low as soon as the PLL is locked or timed out.
    /// The command SetFs() is used to set the device in the frequency synthesis mode where the PLL is locked to the carrier
    /// frequency. This mode is used for test purposes of the PLL and can be considered as an intermediate mode. It is
    /// automatically reached when going from STDBY_RC mode to TX mode or RX mode.
    Fs = 2,
    /// The inner value is the timeout, in ms.
    Tx = 3,
    Rx = 4,
    RxDc = 5,
    Cad = 6,
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
        const NONE                  = 0b0000000000;
        const TX_DONE               = 0b0000000001;
        const RX_DONE               = 0b0000000010;
        const PREAMBLE_DETECTED     = 0b0000000100;
        const SYNCWORD_VALID        = 0b0000001000;
        const HEADER_VALID          = 0b0000010000;
        const HEADER_ERROR          = 0b0000100000;
        const CRC_ERROR             = 0b0001000000;
        const CAD_DONE              = 0b0010000000;
        const CAD_ACTIVITY_DETECTED = 0b0100000000;
        const TIMEOUT               = 0b1000000000;
        const ALL                   = 0xFFFF;
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
#[derive(Clone, Copy, PartialEq, Eq)]
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
pub enum TcxoCtrlVoltage {
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

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
#[allow(non_camel_case_types, dead_code)]
pub enum Standby {
    Rc  = 0,
    Osc = 1,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
#[allow(non_camel_case_types, dead_code)]
pub enum CadSymbol {
    Symbol1   = 0,
    Symbol2   = 1,
    Symbol4   = 2,
    Symbol8   = 3,
    Symbol16  = 4,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Format, Debug)]
#[allow(non_camel_case_types, dead_code)]
pub enum CadExitMode {
    CadOnly  = 0x0, // Go to StandbyRc
    CadRx    = 0x1, // Go to Rx if activity detected
    CadLbt   = 0x10,// Listen Before Talk (Tx if no activity).
}