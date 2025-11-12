use embassy_nrf::{
    interrupt::{
        self,
        typelevel::{self, Interrupt},
    },
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, waitqueue::AtomicWaker,
};

use crate::radio::{
    Packet, DONGLE_ADDRESS, DONGLE_PREFIX, KEYBOARD_ADDRESS, LEFT_PREFIX, RIGHT_PREFIX,
};

pub struct InterruptHandler {}

static TRAD_STATE: AtomicWaker = AtomicWaker::new();
const BUFFER_SIZE: usize = 32;
static mut RX_ID: [u8; 8] = [0u8; 8];

static mut CURRENT_PACKET: Packet = Packet::default();

enum RadioState {
    Disabled,
    Tx,
    TxAck,
    Rx,
    RxAck,
}

static mut RADIO_STATE: RadioState = RadioState::Disabled;
pub struct TradInterruptHandler {}

impl interrupt::typelevel::Handler<typelevel::RADIO> for TradInterruptHandler {
    unsafe fn on_interrupt() {
        let r = embassy_nrf::pac::RADIO;
        match RADIO_STATE {
            RadioState::Tx => {}
            RadioState::TxAck => todo!(),
            RadioState::Rx => todo!(),
            RadioState::RxAck => todo!(),
            RadioState::Disabled => {}
        }
    }
}

#[derive(Clone, Copy)]
pub struct Addresses {
    pub base: [u32; 2],
    pub prefix: [[u8; 4]; 2],
}

impl Default for Addresses {
    fn default() -> Self {
        let mut res = Self {
            base: Default::default(),
            prefix: Default::default(),
        };
        res.base[0] = DONGLE_ADDRESS;
        res.base[1] = KEYBOARD_ADDRESS;
        res.prefix[0][0] = DONGLE_PREFIX;
        res.prefix[0][1] = LEFT_PREFIX;
        res.prefix[0][2] = RIGHT_PREFIX;
        res
    }
}

pub struct TradRadio<'d> {
    _radio: Peri<'d, embassy_nrf::peripherals::RADIO>,
    tx_addreses: u8,
    rx_addresses: u32,
}

impl<'d> TradRadio<'d> {
    pub fn new(
        _radio: Peri<'d, embassy_nrf::peripherals::RADIO>,
        _irq: impl interrupt::typelevel::Binding<
            embassy_nrf::interrupt::typelevel::RADIO,
            TradInterruptHandler,
        >,
        addresses: Addresses,
    ) -> Self {
        let r = embassy_nrf::pac::RADIO;

        r.power().write(|w| w.set_power(false));
        r.power().write(|w| w.set_power(true));

        r.mode()
            .write(|w| w.set_mode(embassy_nrf::pac::radio::vals::Mode::NRF_1MBIT));

        r.pcnf0().write(|w| {
            w.set_lflen(8);
            w.set_s0len(false);
            w.set_s1len(0);
            w.set_s1incl(embassy_nrf::pac::radio::vals::S1incl::AUTOMATIC);
            w.set_plen(embassy_nrf::pac::radio::vals::Plen::_8BIT);
        });

        r.pcnf1().write(|w| {
            w.set_maxlen(BUFFER_SIZE as u8);
            w.set_statlen(0);
            w.set_balen(4);
            w.set_endian(embassy_nrf::pac::radio::vals::Endian::LITTLE);
        });

        r.base0().write_value(addresses.base[0]);
        r.base1().write_value(addresses.base[1]);
        r.prefix0()
            .write(|w| w.0 = u32::from_le_bytes(addresses.prefix[0]));
        r.prefix1()
            .write(|w| w.0 = u32::from_le_bytes(addresses.prefix[1]));

        r.crccnf().write(|w| {
            w.set_len(embassy_nrf::pac::radio::vals::Len::TWO);
            w.set_skipaddr(embassy_nrf::pac::radio::vals::Skipaddr::INCLUDE);
        });
        r.crcpoly().write(|w| w.set_crcpoly(0x1_1021));
        r.crcinit().write(|w| w.set_crcinit(0x0000_FFFF));

        r.modecnf0().write(|w| {
            w.set_ru(embassy_nrf::pac::radio::vals::Ru::FAST);
            w.set_dtx(embassy_nrf::pac::radio::vals::Dtx::B0);
        });

        r.frequency().write(|w| {
            w.set_frequency(80);
        });

        embassy_nrf::interrupt::typelevel::RADIO::unpend();

        unsafe {
            embassy_nrf::interrupt::typelevel::RADIO::enable();
        }

        Self {
            _radio,
            rx_addresses: 0,
            tx_addreses: 0,
        }
    }
}
