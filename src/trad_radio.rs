use core::{
    cell::{Cell, UnsafeCell},
    ptr::null,
    sync::atomic::{compiler_fence, AtomicBool},
    task::Poll,
};

use defmt::info;
use embassy_nrf::{
    interrupt::{
        self,
        typelevel::{self, Interrupt},
    },
    pac::radio::regs::{Rxaddresses, Txaddress},
    Peri,
};
use embassy_sync::{
    blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel, waitqueue::AtomicWaker,
};
use embassy_time::{Duration, Instant};

use crate::radio::{
    LogInfo, Packet, PacketType, DONGLE_ADDRESS, DONGLE_PREFIX, KEYBOARD_ADDRESS, LEFT_PREFIX,
    RIGHT_PREFIX,
};

pub struct InterruptHandler {}

const BUFFER_SIZE: usize = 32;
static TRAD_STATE: AtomicWaker = AtomicWaker::new();
static mut START: u64 = 0;
static mut COUNT: u32 = 0;

enum RadioState {
    Disabled,
    Tx,
    TxAck,
    Rx,
    RxAck,
}

static mut CURRENT_PACKET: Packet = Packet::default();
static mut TX_ID: u8 = 0;
static mut RX_ID: u8 = 0;
static ACTIVE: AtomicBool = AtomicBool::new(false);
pub struct TradInterruptHandler {}
static mut RADIO_STATE: RadioState = RadioState::Disabled;

static CHAN: Channel<CriticalSectionRawMutex, LogInfo, 5> = Channel::new();
static P_CHAN: Channel<CriticalSectionRawMutex, Packet, 5> = Channel::new();

impl typelevel::Handler<typelevel::RADIO> for TradInterruptHandler {
    unsafe fn on_interrupt() {
        static mut ACK_PACKET: Packet = Packet::default();
        let r = embassy_nrf::pac::RADIO;
        let t = embassy_nrf::pac::TIMER0;
        // info!("Radio Event!!!");
        match RADIO_STATE {
            RadioState::Disabled => {}
            RadioState::Tx => {
                if r.events_disabled().read() != 0 {
                    r.events_disabled().write_value(0);
                    r.packetptr().write_value(ACK_PACKET.buffer.as_ptr() as u32);
                    RADIO_STATE = RadioState::TxAck;
                    t.tasks_start().write_value(1);
                    compiler_fence(core::sync::atomic::Ordering::Release);
                    r.tasks_rxen().write_value(1);
                }
            }
            RadioState::TxAck => {
                if r.events_disabled().read() != 0 {
                    r.events_disabled().write_value(0);
                    if r.events_crcok().read() != 0 {
                        r.events_crcok().write_value(0);
                        if ACK_PACKET.packet_type().unwrap() == PacketType::Ack
                            && ACK_PACKET.id() == TX_ID
                        {
                            TX_ID += 1;
                            // ACTIVE.store(false, core::sync::atomic::Ordering::Release);
                            RADIO_STATE = RadioState::Disabled;
                            // TRAD_STATE.wake();
                            t.tasks_stop().write_value(1);
                            let _ = CHAN.try_send(LogInfo {
                                retranmisisons: COUNT,
                                time_elapsed: Duration::from_ticks(
                                    Instant::now().as_ticks() - START,
                                ),
                            });
                        }
                    } else {
                        RADIO_STATE = RadioState::Tx;
                        r.packetptr()
                            .write_value(CURRENT_PACKET.buffer.as_ptr() as u32);
                        COUNT += 1;
                        START = Instant::now().as_ticks();
                        compiler_fence(core::sync::atomic::Ordering::Release);
                        t.tasks_stop().write_value(1);
                        r.tasks_txen().write_value(1);
                    }
                }
            }
            RadioState::Rx => {
                if r.events_disabled().read() != 0 {
                    r.events_disabled().write_value(0);
                    if r.events_crcok().read() != 0 {
                        r.events_crcok().write_value(0);
                        if CURRENT_PACKET.packet_type().unwrap() == PacketType::Data {
                            RADIO_STATE = RadioState::RxAck;
                            ACK_PACKET.set_len(1);
                            ACK_PACKET.set_type(PacketType::Ack);
                            r.packetptr().write_value(ACK_PACKET.buffer.as_ptr() as u32);
                            compiler_fence(core::sync::atomic::Ordering::Release);
                            r.tasks_txen().write_value(1);
                        } else {
                            r.tasks_rxen().write_value(1);
                        }
                    } else {
                        r.tasks_rxen().write_value(1);
                    }
                }
            }
            RadioState::RxAck => {
                if r.events_disabled().read() != 0 {
                    r.events_disabled().write_value(0);
                    if CURRENT_PACKET.id() != RX_ID {
                        RADIO_STATE = RadioState::Disabled;
                        RX_ID = CURRENT_PACKET.id();
                        let _ = P_CHAN.try_send(CURRENT_PACKET);
                    } else {
                        RADIO_STATE = RadioState::Rx;
                        r.tasks_rxen().write_value(1);
                    }
                }
            }
        }
    }
}

pub struct RadioTimerInterrupt;

impl typelevel::Handler<typelevel::TIMER0> for RadioTimerInterrupt {
    unsafe fn on_interrupt() {
        let t = embassy_nrf::pac::TIMER0;
        let r = embassy_nrf::pac::RADIO;
        match RADIO_STATE {
            RadioState::Disabled => {
                if t.events_compare(0).read() != 0 {
                    t.events_compare(0).write_value(0);
                }
                t.tasks_stop().write_value(1);
                t.tasks_clear().write_value(1);
            }
            RadioState::Tx => {
                // info!("TX Timer Fire!");
                if t.events_compare(0).read() != 0 {
                    t.events_compare(0).write_value(0);
                }
                t.tasks_stop().write_value(1);
                t.tasks_clear().write_value(1);
            }
            RadioState::TxAck => {
                // info!("TX Timer Ack Fire!");
                t.tasks_stop().write_value(1);
                t.tasks_clear().write_value(1);
                if t.events_compare(0).read() != 0 {
                    t.events_compare(0).write_value(0);
                    r.tasks_disable().write_value(1);
                    RADIO_STATE = RadioState::Disabled;
                    while r.state().read().state()
                        != embassy_nrf::radio::ieee802154::RadioState::DISABLED
                    {
                    }
                    r.events_disabled().write_value(0);

                    RADIO_STATE = RadioState::Tx;
                    r.packetptr()
                        .write_value(CURRENT_PACKET.buffer.as_ptr() as u32);
                    COUNT += 1;
                    START = Instant::now().as_ticks();
                    compiler_fence(core::sync::atomic::Ordering::Release);
                    r.tasks_txen().write_value(1);
                }
            }
            RadioState::Rx => todo!(),
            RadioState::RxAck => todo!(),
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
        _timer: Peri<'d, embassy_nrf::peripherals::TIMER0>,
        _irq: impl typelevel::Binding<embassy_nrf::interrupt::typelevel::RADIO, TradInterruptHandler>,
        _irq_t: impl typelevel::Binding<embassy_nrf::interrupt::typelevel::TIMER0, RadioTimerInterrupt>,
        addresses: Addresses,
    ) -> Self {
        let t = embassy_nrf::pac::TIMER0;
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

        r.shorts().write(|w| {
            w.set_ready_start(true);
            w.set_end_disable(true);
        });

        embassy_nrf::interrupt::typelevel::RADIO::unpend();

        unsafe {
            embassy_nrf::interrupt::typelevel::RADIO::enable();
        }
        r.intenset().write(|w| w.set_disabled(true));

        t.mode()
            .write(|w| w.set_mode(embassy_nrf::pac::timer::vals::Mode::TIMER));
        t.bitmode()
            .write(|w| w.set_bitmode(embassy_nrf::pac::timer::vals::Bitmode::_32BIT));
        t.prescaler().write(|w| w.set_prescaler(4));
        t.cc(0).write_value(300);

        embassy_nrf::interrupt::typelevel::TIMER0::unpend();
        unsafe {
            embassy_nrf::interrupt::typelevel::TIMER0::enable();
        }

        t.intenset().write(|w| w.set_compare(0, true));
        Self {
            _radio,
            rx_addresses: 0,
            tx_addreses: 0,
        }
    }

    pub fn set_tx_addresses(&mut self, f: impl FnOnce(&mut Txaddress)) {
        let r = embassy_nrf::pac::RADIO;
        r.txaddress().write(f);
        self.tx_addreses = r.txaddress().read().txaddress();
    }

    pub fn set_rx_addresses(&mut self, f: impl FnOnce(&mut Rxaddresses)) {
        let r = embassy_nrf::pac::RADIO;
        r.rxaddresses().write(f);
        self.rx_addresses = r.rxaddresses().read().0;
    }

    pub async fn receive_packet(&mut self) -> Packet {
        let r = embassy_nrf::pac::RADIO;
        cortex_m::interrupt::free(|_cs| unsafe {
            RADIO_STATE = RadioState::Rx;
            r.packetptr()
                .write_value(CURRENT_PACKET.buffer.as_ptr() as u32);
            compiler_fence(core::sync::atomic::Ordering::Release);
            r.tasks_rxen().write_value(1);
        });
        P_CHAN.receive().await
    }

    pub async fn send_packet(&mut self, packet: Packet) -> LogInfo {
        let r = embassy_nrf::pac::RADIO;
        // if ACTIVE.load(core::sync::atomic::Ordering::Acquire) {
        //     core::future::poll_fn(|cx| {
        //         TRAD_STATE.register(cx.waker());
        //         if !ACTIVE.load(core::sync::atomic::Ordering::Acquire) {
        //             Poll::Ready(())
        //         } else {
        //             Poll::Pending
        //         }
        //     })
        //     .await;
        // }
        // ACTIVE.store(true, core::sync::atomic::Ordering::Release);
        cortex_m::interrupt::free(|_cs| unsafe {
            CURRENT_PACKET = packet;
            CURRENT_PACKET.set_id(TX_ID);
            CURRENT_PACKET.set_type(PacketType::Data);
            compiler_fence(core::sync::atomic::Ordering::Release);
            r.packetptr()
                .write_value(CURRENT_PACKET.buffer.as_ptr() as u32);
            RADIO_STATE = RadioState::Tx;
            START = Instant::now().as_ticks();
            COUNT = 0;
            compiler_fence(core::sync::atomic::Ordering::Release);
            r.tasks_txen().write_value(1);
        });
        CHAN.receive().await
    }
}
