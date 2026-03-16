use bitfield::bitfield;
use defmt::{info, Format, Formatter};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use crate::cursor::Cursor;
use crate::{ordinal, proto};
use crate::proto::{ReadWire, WriteWire, Wire, FromWire, ToWire};
use crate::varint::v32;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: NodeId = NodeId(0);
    pub const BROADCAST: NodeId = NodeId(u32::MAX);
}

impl Format for NodeId {
    fn format(&self, fmt: Formatter<'_>) {
        if *self == NodeId::NONE {
            defmt::write!(fmt, "NONE");
        } else if *self == NodeId::BROADCAST {
            defmt::write!(fmt, "EVERYONE");
        } else {
            defmt::write!(fmt, "!{:08X}", self.0);
        }
    }
}

#[derive(Format)]
pub struct MestasticHeader {
    pub to: NodeId,
    pub from: NodeId,
    pub packet_id: u32,
    pub flags: PacketFlags,
    pub channel: u8,
    pub next_hop: u8,
    pub relay_node: u8,
}

impl MestasticHeader {
    pub fn read(cursor: &mut Cursor<&mut [u8]>) -> Self {
        Self {
            to: NodeId(cursor.read_u32_le()),
            from: NodeId(cursor.read_u32_le()),
            packet_id: cursor.read_u32_le(),
            flags: PacketFlags(cursor.read_u8()),
            channel: cursor.read_u8(),
            next_hop: cursor.read_u8(),
            relay_node: cursor.read_u8(),
        }
    }

    pub fn write(&self, cursor: &mut Cursor<&mut [u8]>) {
        cursor.write_u32_le(self.to.0);
        cursor.write_u32_le(self.from.0);
        cursor.write_u32_le(self.packet_id);
        cursor.write_u8(self.flags.0);
        cursor.write_u8(self.channel);
        cursor.write_u8(self.next_hop);
        cursor.write_u8(self.relay_node);
    }

    pub fn is_broadcast(&self) -> bool {
        self.to == NodeId(u32::MAX)
    }
}

bitfield! {
    pub struct PacketFlags(u8);
    u8;
    pub get_hop_limit, set_hop_limit: 3, 0;
    pub get_want_ack, set_want_ack: 4, 3;
    pub get_via_mqtt, set_via_mqtt: 5, 4;
    pub get_hop_start, set_hop_start: 8, 5;
}

impl PacketFlags {
    pub fn new(hop_limit: u8, want_ack: bool, via_mqtt: bool, hop_start: u8) -> Self {
        let mut this = Self(0);
        this.set_hop_limit(hop_limit);
        this.set_want_ack(want_ack as u8);
        this.set_via_mqtt(via_mqtt as u8);
        this.set_hop_start(hop_start);
        this
    }
}

impl Format for PacketFlags {
    fn format(&self, fmt: Formatter<'_>) {
        defmt::write!(fmt, "{{ hop_limit={}, want_ack={}, via_mqtt={}, hop_start={} }}", self.get_hop_limit(), self.get_want_ack() != 0, self.get_via_mqtt() != 0, self.get_hop_start());
    }
}

#[repr(packed)]
pub struct Nonce {
    pub packet_id: u32,
    pub extra: u32,
    pub from: NodeId,
    pub pad: u32
}

impl Nonce {
    pub fn as_ccm_bytes(&self) -> &[u8; 13] {
        unsafe { core::mem::transmute(self) }
    }

    pub fn as_ctr_bytes(&self) -> &[u8; 16] {
        unsafe { core::mem::transmute(self) }
    }
}

#[repr(u32)]
#[derive(FromPrimitive, Format, Default, Clone, Copy, PartialEq, Eq)]
pub enum PortNum {
    #[default]
    UnknownApp = 0,
    TextMessageApp = 1,
    RemoteHardwareApp = 2,
    PositionApp = 3,
    NodeInfoApp = 4,
    RoutingApp = 5,
    AdminApp = 6,
    TextMessageCompressedApp = 7,
    WaypointApp = 8,
    AudioApp = 9,
    DetectionSensorApp = 10,
    AlertApp = 11,
    KeyVerificationApp = 12,
    ReplyApp = 32,
    IpTunnelApp = 33,
    PaxCounterApp = 34,
    StoreForwardPlusPlusApp = 35,
    NodeStatusApp = 36,
    SerialApp = 64,
    StoreForwardApp = 65,
    RangeTestApp = 66,
    TelemetryApp = 67,
    ZpsApp = 68,
    SimulatorApp = 69,
    TracerouteApp = 70,
    NeighborInfoApp = 71,
    AtakPlugin = 72,
    MapReportApp = 73,
    PowerStressApp = 74,
    ReticulumTunnelApp = 76,
    CayenneApp = 77,
    PrivateApp = 256,
    AtakForwarder = 257,
    Max = 511,
}

#[repr(u32)]
#[derive(FromPrimitive, Format, Default, Clone, Copy, PartialEq, Eq)]
pub enum HardwareModel {
    #[default]
    Unset = 0,
    TloraV2 = 1,
    TloraV1 = 2,
    TloraV211p6 = 3,
    TBeam = 4,
    /*
     * The original heltec WiFi_Lora_32_V2, which had battery voltage sensing hooked to GPIO 13
     * (see HELTEC_V2 for the new version).
     */
    HeltecV20 = 5,
    TBeamV0p7 = 6,
    TEcho = 7,
    TloraV11p3 = 8,
    RAK4631 = 9,
    /*
     * The new version of the heltec WiFi_Lora_32_V2 board that has battery sensing hooked to GPIO 37.
     * Sadly they did not update anything on the silkscreen to identify this board
     */
    HeltecV21 = 10,
    /*
     * Ancient heltec WiFi_Lora_32 board
     */
    HeltecV1 = 11,
    /*
     * New T-BEAM with ESP32-S3 CPU
     */
    LilygoTBeamS3Core = 12,
    /*
     * RAK WisBlock ESP32 core: https://docs.rakwireless.com/Product-Categories/WisBlock/RAK11200/Overview/
     */
    RAK11200 = 13,

    /*
     * B&Q Consulting Nano Edition G1: https://uniteng.com/wiki/doku.php?id=meshtastic:nano
     */
    NanoG1 = 14,
    TloraV211p8 = 15,
    TloraT3S3 = 16,

    /*
     * B&Q Consulting Nano G1 Explorer: https://wiki.uniteng.com/en/meshtastic/nano-g1-explorer
     */
    NanoG1Explorer = 17,

    /*
     * B&Q Consulting Nano G2 Ultra: https://wiki.uniteng.com/en/meshtastic/nano-g2-ultra
     */
    NanoG2Ultra = 18,

    /*
     * LoRAType device: https://loratype.org/
     */
    LoraType = 19,

    /*
     * wiphone https://www.wiphone.io/
     */
    WIPHONE = 20,

    /*
     * WIO Tracker WM1110 family from Seeed Studio. Includes wio-1110-tracker and wio-1110-sdk
     */
    WioWm1110 = 21,

    /*
     * RAK2560 Solar base station based on RAK4630
     */
    RAK2560 = 22,

    /*
     * Heltec HRU-3601: https://heltec.org/project/hru-3601/
     */
    HeltecHru3601 = 23,

    /*
     * Heltec Wireless Bridge
     */
    HeltecWirelessBridge = 24,

    /*
     * B&Q Consulting Station Edition G1: https://uniteng.com/wiki/doku.php?id=meshtastic:station
     */
    StationG1 = 25,

    /*
     * RAK11310 (RP2040 + SX1262)
     */
    RAK11310 = 26,

    /*
     * Makerfabs SenseLoRA Receiver (RP2040 + RFM96)
     */
    SenseloraRp2040 = 27,

    /*
     * Makerfabs SenseLoRA Industrial Monitor (ESP32-S3 + RFM96)
     */
    SenseloraS3 = 28,

    /*
     * Canary Radio Company - CanaryOne: https://canaryradio.io/products/canaryone
     */
    CANARYONE = 29,

    /*
     * Waveshare RP2040 LoRa - https://www.waveshare.com/rp2040-lora.htm
     */
    Rp2040Lora = 30,

    /*
     * B&Q Consulting Station G2: https://wiki.uniteng.com/en/meshtastic/station-g2
     */
    StationG2 = 31,

    /*
     * ---------------------------------------------------------------------------
     * Less common/prototype boards listed here (needs one more byte over the air)
     * ---------------------------------------------------------------------------
     */
    LoraRelayV1 = 32,

    /*
     * T-Echo Plus device from LilyGo
     */
    TEchoPlus = 33,
    PPR = 34,
    GenieBlocks = 35,
    Nrf52Unknown = 36,
    Portduino = 37,

    /*
     * The simulator built into the android app
     */
    AndroidSim = 38,

    /*
     * Custom DIY device based on @NanoVHF schematics: https://github.com/NanoVHF/Meshtastic-DIY/tree/main/Schematics
     */
    DiyV1 = 39,

    /*
     * nRF52840 Dongle : https://www.nordicsemi.com/Products/Development-hardware/nrf52840-dongle/
     */
    Nrf52840Pca10059 = 40,

    /*
     * Custom Disaster Radio esp32 v3 device https://github.com/sudomesh/disaster-radio/tree/master/hardware/board_esp32_v3
     */
    DrDev = 41,

    /*
     * M5 esp32 based MCU modules with enclosure, TFT and LORA Shields. All Variants (Basic, Core, Fire, Core2, CoreS3, Paper) https://m5stack.com/
     */
    M5STACK = 42,

    /*
     * New Heltec LoRA32 with ESP32-S3 CPU
     */
    HeltecV3 = 43,

    /*
     * New Heltec Wireless Stick Lite with ESP32-S3 CPU
     */
    HeltecWslV3 = 44,

    /*
     * New BETAFPV ELRS Micro TX Module 2.4G with ESP32 CPU
     */
    Betafpv2400Tx = 45,

    /*
     * BetaFPV ExpressLRS "Nano" TX Module 900MHz with ESP32 CPU
     */
    Betafpv900NanoTx = 46,

    /*
     * Raspberry Pi Pico (W) with Waveshare SX1262 LoRa Node Module
     */
    RpiPico = 47,

    /*
     * Heltec Wireless Tracker with ESP32-S3 CPU, built-in GPS, and TFT
     * Newer V1.1, version is written on the PCB near the display.
     */
    HeltecWirelessTracker = 48,

    /*
     * Heltec Wireless Paper with ESP32-S3 CPU and E-Ink display
     */
    HeltecWirelessPaper = 49,

    /*
     * LilyGo T-Deck with ESP32-S3 CPU, Keyboard and IPS display
     */
    TDeck = 50,

    /*
     * LilyGo T-Watch S3 with ESP32-S3 CPU and IPS display
     */
    TWatchS3 = 51,

    /*
     * Bobricius Picomputer with ESP32-S3 CPU, Keyboard and IPS display
     */
    PicomputerS3 = 52,

    /*
     * Heltec HT-CT62 with ESP32-C3 CPU and SX1262 LoRa
     */
    HeltecHt62 = 53,

    /*
     * EBYTE SPI LoRa module and ESP32-S3
     */
    EbyteEsp32S3 = 54,

    /*
     * Waveshare ESP32-S3-PICO with PICO LoRa HAT and 2.9inch e-Ink
     */
    Esp32S3Pico = 55,

    /*
     * CircuitMess Chatter 2 LLCC68 Lora Module and ESP32 Wroom
     * Lora module can be swapped out for a Heltec RA-62 which is "almost" pin compatible
     * with one cut and one jumper Meshtastic works
     */
    Chatter2 = 56,

    /*
     * Heltec Wireless Paper, With ESP32-S3 CPU and E-Ink display
     * Older "V1.0" Variant, has no "version sticker"
     * E-Ink model is DEPG0213BNS800
     * Tab on the screen protector is RED
     * Flex connector marking is FPC-7528B
     */
    HeltecWirelessPaperV10 = 57,

    /*
     * Heltec Wireless Tracker with ESP32-S3 CPU, built-in GPS, and TFT
     * Older "V1.0" Variant
     */
    HeltecWirelessTrackerV10 = 58,

    /*
     * unPhone with ESP32-S3, TFT touchscreen,  LSM6DS3TR-C accelerometer and gyroscope
     */
    UnPhone = 59,

    /*
     * Teledatics TD-LORAC NRF52840 based M.2 LoRA module
     * Compatible with the TD-WRLS development board
     */
    TdLorac = 60,

    /*
     * CDEBYTE EoRa-S3 board using their own MM modules, clone of LILYGO T3S3
     */
    CdebyteEoraS3 = 61,

    /*
     * Adafruit NRF52840 feather express with SX1262, SSD1306 OLED and NEO6M GPS
     */
    TwcMeshV4 = 62,

    /*
     * Promicro NRF52840 with SX1262/LLCC68, SSD1306 OLED and NEO6M GPS
     */
    Nrf52PromicroDiy = 63,

    /*
     * RadioMaster 900 Bandit Nano, https://www.radiomasterrc.com/products/bandit-nano-expresslrs-rf-module
     * ESP32-D0WDQ6 With SX1276/SKY66122, SSD1306 OLED and No GPS
     */
    Radiomaster900BanditNano = 64,

    /*
     * Heltec Capsule Sensor V3 with ESP32-S3 CPU, Portable LoRa device that can replace GNSS modules or sensors
     */
    HeltecCapsuleSensorV3 = 65,

    /*
     * Heltec Vision Master T190 with ESP32-S3 CPU, and a 1.90 inch TFT display
     */
    HeltecVisionMasterT190 = 66,

    /*
     * Heltec Vision Master E213 with ESP32-S3 CPU, and a 2.13 inch E-Ink display
     */
    HeltecVisionMasterE213 = 67,

    /*
     * Heltec Vision Master E290 with ESP32-S3 CPU, and a 2.9 inch E-Ink display
     */
    HeltecVisionMasterE290 = 68,

    /*
     * Heltec Mesh Node T114 board with nRF52840 CPU, and a 1.14 inch TFT display, Ultimate low-power design,
     * specifically adapted for the Meshtatic project
     */
    HeltecMeshNodeT114 = 69,

    /*
     * Sensecap Indicator from Seeed Studio. ESP32-S3 device with TFT and RP2040 coprocessor
     */
    SensecapIndicator = 70,

    /*
     * Seeed studio T1000-E tracker card. NRF52840 w/ LR1110 radio, GPS, button, buzzer, and sensors.
     */
    TrackerT1000E = 71,

    /*
     * RAK3172 STM32WLE5 Module (https://store.rakwireless.com/products/wisduo-lpwan-module-rak3172)
     */
    RAK3172 = 72,

    /*
     * Seeed Studio Wio-E5 (either mini or Dev kit) using STM32WL chip.
     */
    WioE5 = 73,

    /*
     * RadioMaster 900 Bandit, https://www.radiomasterrc.com/products/bandit-expresslrs-rf-module
     * SSD1306 OLED and No GPS
     */
    Radiomaster900Bandit = 74,

    /*
     * Minewsemi ME25LS01 (ME25LE01_V1.0). NRF52840 w/ LR1110 radio, buttons and leds and pins.
     */
    ME25LS01_4Y10TD = 75,

    /*
     * Adafruit Feather RP2040 with RFM95 LoRa Radio RFM95 with SX1272, SSD1306 OLED
     * https://www.adafruit.com/product/5714
     * https://www.adafruit.com/product/326
     * https://www.adafruit.com/product/938
     *  ^^^ short A0 to switch to I2C address 0x3C
     *
     */
    Rp2040FeatherRfm95 = 76,

    /* M5 esp32 based MCU modules with enclosure, TFT and LORA Shields. All Variants (Basic, Core, Fire, Core2, CoreS3, Paper) https://m5stack.com/ */
    M5stackCorebasic = 77,
    M5stackCore2 = 78,

    /* Pico2 with Waveshare Hat, same as Pico */
    RpiPico2 = 79,

    /* M5 esp32 based MCU modules with enclosure, TFT and LORA Shields. All Variants (Basic, Core, Fire, Core2, CoreS3, Paper) https://m5stack.com/ */
    M5stackCores3 = 80,

    /* Seeed XIAO S3 DK*/
    SeeedXiaoS3 = 81,

    /*
     * Nordic nRF52840+Semtech SX1262 LoRa BLE Combo Module. nRF52840+SX1262 MS24SF1
     */
    MS24SF1 = 82,

    /*
     * Lilygo TLora-C6 with the new ESP32-C6 MCU
     */
    TloraC6 = 83,

    /*
     * WisMesh Tap
     * RAK-4631 w/ TFT in injection modled case
     */
    WismeshTap = 84,

    /*
     * Similar to PORTDUINO but used by Routastic devices, this is not any
     * particular device and does not run Meshtastic's code but supports
     * the same frame format.
     * Runs on linux, see https://github.com/Jorropo/routastic
     */
    ROUTASTIC = 85,

    /*
     * Mesh-Tab, esp32 based
     * https://github.com/valzzu/Mesh-Tab
     */
    MeshTab = 86,

    /*
     * MeshLink board developed by LoraItalia. NRF52840, eByte E22900M22S (Will also come with other frequencies), 25w MPPT solar charger (5v,12v,18v selectable), support for gps, buzzer, oled or e-ink display, 10 gpios, hardware watchdog
     * https://www.loraitalia.it
     */
    MESHLINK = 87,

    /*
     * Seeed XIAO nRF52840 + Wio SX1262 kit
     */
    XiaoNrf52Kit = 88,

    /*
     * Elecrow ThinkNode M1 & M2
     * https://www.elecrow.com/wiki/ThinkNode-M1_Transceiver_Device(Meshtastic)_Power_By_nRF52840.html
     * https://www.elecrow.com/wiki/ThinkNode-M2_Transceiver_Device(Meshtastic)_Power_By_NRF52840.html (this actually uses ESP32-S3)
     */
    ThinknodeM1 = 89,
    ThinknodeM2 = 90,

    /*
     * Lilygo T-ETH-Elite
     */
    TEthElite = 91,

    /*
     * Heltec HRI-3621 industrial probe
     */
    HeltecSensorHub = 92,

    /*
     * Muzi Works Muzi-Base device
     */
    MuziBase = 93,

    /*
     * Heltec Magnetic Power Bank with Meshtastic compatible
     */
    HeltecMeshPocket = 94,

    /*
     * Seeed Solar Node
     */
    SeeedSolarNode = 95,

    /*
     * NomadStar Meteor Pro https://nomadstar.ch/
     */
    NomadstarMeteorPro = 96,

    /*
     * Elecrow CrowPanel Advance models, ESP32-S3 and TFT with SX1262 radio plugin
     */
    CROWPANEL = 97,

    /*
     * Lilygo LINK32 board with sensors
     */
    Link32 = 98,

    /*
     * Seeed Tracker L1
     */
    SeeedWioTrackerL1 = 99,

    /*
     * Seeed Tracker L1 EINK driver
     */
    SeeedWioTrackerL1Eink = 100,

    /*
     * Muzi Works R1 Neo
     */
    MuziR1Neo = 101,

    /*
     * Lilygo T-Deck Pro
     */
    TDeckPro = 102,

    /*
     * Lilygo TLora Pager
     */
    TLoraPager = 103,

    /*
     * M5Stack Reserved
     */
    M5stackReserved = 104, // 0x68

    /*
     * RAKwireless WisMesh Tag
     */
    WismeshTag = 105,
    /*
     * RAKwireless WisBlock Core RAK3312 https://docs.rakwireless.com/product-categories/wisduo/rak3112-module/overview/
     */
    RAK3312 = 106,
    /*
     * Elecrow ThinkNode M5 https://www.elecrow.com/wiki/ThinkNode_M5_Meshtastic_LoRa_Signal_Transceiver_ESP32-S3.html
     */
    ThinknodeM5 = 107,
    /*
     * MeshSolar is an integrated power management and communication solution designed for outdoor low-power devices.
     * https://heltec.org/project/meshsolar/
     */
    HeltecMeshSolar = 108,
    /*
     * Lilygo T-Echo Lite
     */
    TEchoLite = 109,
    /*
     * New Heltec LoRA32 with ESP32-S3 CPU
     */
    HeltecV4 = 110,
    /*
     * M5Stack C6L
     */
    M5stackC6l = 111,
    /*
     * M5Stack Cardputer Adv
     */
    M5stackCardputerAdv = 112,
    /*
     * ESP32S3 main controller with GPS and TFT screen.
     */
    HeltecWirelessTrackerV2 = 113,
    /*
     * LilyGo T-Watch Ultra
     */
    TWatchUltra = 114,
    /*
     * Elecrow ThinkNode M3
     */
    ThinknodeM3 = 115,
    /*
     * RAK WismeshTapV2 with ESP32-S3 CPU
     */
    WisMeshTapV2 = 116,
    /*
     * RAK3401
     */
    Rak3401 = 117,
    /*
     * RAK6421 Hat+
     */
    Rak6421 = 118,
    /*
     * Elecrow ThinkNode M4
     */
    ThinknodeM4 = 119,
    /*
     * Elecrow ThinkNode M6
     */
    ThinknodeM6 = 120,
    /*
     * Elecrow Meshstick 1262
     */
    Meshstick1262 = 121,
    /*
     * LilyGo T-Beam 1W
     */
    TBeam1Watt = 122,
    /*
     * LilyGo T5 S3 ePaper Pro (V1 and V2)
     */
    T5S3EpaperPro = 123,
    /*
     * LilyGo T-Beam BPF (144-148Mhz)
     */
    TBeamBpf = 124,
    /*
     * LilyGo T-Mini E-paper S3 Kit
     */
    MiniEpaperS3 = 125,
    /*
     * LilyGo T-Display S3 Pro LR1121
     */
    TdisplayS3Pro = 126,
    /*
     * ------------------------------------------------------------------------------------------------------------------------------------------
     * Reserved ID For developing private Ports. These will show up in live traffic sparsely, so we can use a high number. Keep it within 8 bits.
     * ------------------------------------------------------------------------------------------------------------------------------------------
     */
    PrivateHw = 255,
}

#[repr(u32)]
#[derive(FromPrimitive, Format, Default, Clone, Copy, PartialEq, Eq)]
pub enum DeviceRole {
    /*
     * Description: App connected or stand alone messaging device.
     * Technical Details: Default Role
     */
    #[default]
    Client = 0,
    /*
     *  Description: Device that does not forward packets from other devices.
     */
    ClientMute = 1,
    /*
     * Description: Infrastructure node for extending network coverage by relaying messages. Visible in Nodes list.
     * Technical Details: Mesh packets will prefer to be routed over this node. This node will not be used by client apps.
     *   The wifi radio and the oled screen will be put to sleep.
     *   This mode may still potentially have higher power usage due to it's preference in message rebroadcasting on the mesh.
     */
    Router = 2,

    /*
     * Description: Combination of both ROUTER and CLIENT. Not for mobile devices.
     * Deprecated in v2.3.15 because improper usage is impacting public meshes: Use ROUTER or CLIENT instead.
     */
    #[deprecated]
    RouterClient = 3,

    /*
     * Description: Infrastructure node for extending network coverage by relaying messages with minimal overhead. Not visible in Nodes list.
     * Technical Details: Mesh packets will simply be rebroadcasted over this node. Nodes configured with this role will not originate NodeInfo, Position, Telemetry
     *   or any other packet type. They will simply rebroadcast any mesh packets on the same frequency, channel num, spread factor, and coding rate.
     * Deprecated in v2.7.11 because it creates "holes" in the mesh rebroadcast chain.
     */
    #[deprecated]
    Repeater = 4,

    /*
     * Description: Broadcasts GPS position packets as priority.
     * Technical Details: Position Mesh packets will be prioritized higher and sent more frequently by default.
     *   When used in conjunction with power.is_power_saving = true, nodes will wake up,
     *   send position, and then sleep for position.position_broadcast_secs seconds.
     */
    Tracker = 5,

    /*
     * Description: Broadcasts telemetry packets as priority.
     * Technical Details: Telemetry Mesh packets will be prioritized higher and sent more frequently by default.
     *   When used in conjunction with power.is_power_saving = true, nodes will wake up,
     *   send environment telemetry, and then sleep for telemetry.environment_update_interval seconds.
     */
    Sensor = 6,

    /*
     * Description: Optimized for ATAK system communication and reduces routine broadcasts.
     * Technical Details: Used for nodes dedicated for connection to an ATAK EUD.
     *    Turns off many of the routine broadcasts to favor CoT packet stream
     *    from the Meshtastic ATAK plugin -> IMeshService -> Node
     */
    Tak = 7,

    /*
     * Description: Device that only broadcasts as needed for stealth or power savings.
     * Technical Details: Used for nodes that "only speak when spoken to"
     *    Turns all the routine broadcasts but allows for ad-hoc communication
     *    Still rebroadcasts, but with local only rebroadcast mode (known meshes only)
     *    Can be used for clandestine operation or to dramatically reduce airtime / power consumption
     */
    ClientHidden = 8,

    /*
     * Description: Broadcasts location as message to default channel regularly for to assist with device recovery.
     * Technical Details: Used to automatically send a text message to the mesh
     *    with the current position of the device on a frequent interval:
     *    "I'm lost! Position: lat / long"
     */
    LostAndFound = 9,

    /*
     * Description: Enables automatic TAK PLI broadcasts and reduces routine broadcasts.
     * Technical Details: Turns off many of the routine broadcasts to favor ATAK CoT packet stream
     *    and automatic TAK PLI (position location information) broadcasts.
     *    Uses position module configuration to determine TAK PLI broadcast interval.
     */
    TakTracker = 10,

    /*
     * Description: Will always rebroadcast packets, but will do so after all other modes.
     * Technical Details: Used for router nodes that are intended to provide additional coverage
     *    in areas not already covered by other routers, or to bridge around problematic terrain,
     *    but should not be given priority over other routers in order to avoid unnecessaraily
     *    consuming hops.
     */
    RouterLate = 11,

    /*
     * Description: Treats packets from or to favorited nodes as RouterLate, and all other packets as CLIENT.
     * Technical Details: Used for stronger attic/roof nodes to distribute messages more widely
     *    from weaker, indoor, or less-well-positioned nodes. Recommended for users with multiple nodes
     *    where one ClientBase acts as a more powerful base station, such as an attic/roof node.
     */
    ClientBase = 12,
}

ordinal!(PortNum);
ordinal!(HardwareModel);
ordinal!(DeviceRole);

impl<'a> FromWire<'a> for NodeId {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        Self(wire.expect_fixed32(field))
    }
}

impl<'a> ToWire<'a> for NodeId {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == NodeId::NONE { None } else { Some(Wire::Fixed32(self.0)) }
    }

    fn wire_len(&self) -> usize {
        if *self == NodeId::NONE { 0 } else { 4 }
    }
}


proto! {
    pub struct Data<'a> {
        pub port_num: PortNum = 1,
        pub payload: &'a [u8] = 2,
        pub want_response: bool = 3,
        pub dest: NodeId = 4,
        pub source: NodeId = 5,
        pub request_id: u32 = 6,
        pub reply_id: u32 = 7,
        pub emoji: u32 = 8,
        pub bitfield: Option<v32> = 9,
    }
}

proto! {
    pub struct User<'a> {
        pub id: &'a str = 1,
        pub long_name: &'a str = 2,
        pub short_name: &'a str = 3,
        #[deprecated]
        pub macaddr: &'a [u8] = 4,
        pub hw_model: HardwareModel = 5,
        pub is_licensed: bool = 6,
        pub role: DeviceRole = 7,
        pub public_key: &'a [u8] = 8,
        pub is_unmessagable: Option<bool> = 9,
    }
}