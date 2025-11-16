#![no_std]
#![no_main]

use core::{mem, ops::Deref};

use bruh78::{
    radio::{self, LogInfo, Packet, Radio},
    trad_radio::{self, Addresses, TradRadio},
};
use cortex_m_rt::entry;
use defmt::{info, *};
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::join::join;
use embassy_nrf::{
    bind_interrupts,
    config::HfclkSource,
    gpio::Output,
    interrupt::{self, InterruptExt},
    pac,
    peripherals::{self, USBD},
    usb::{self, vbus_detect::HardwareVbusDetect, Driver},
    Peri,
};

use defmt_rtt as _; // global logger
use embassy_nrf as _;
use embassy_time::Timer;
use heapless::Vec;
// time driver
use panic_probe as _;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    RADIO  => trad_radio::TradInterruptHandler;
    TIMER0  => trad_radio::RadioTimerInterrupt;
});

#[embassy_executor::task]
async fn logger_task(usbd: Peri<'static, peripherals::USBD>) {
    let driver = Driver::new(usbd, Irqs, HardwareVbusDetect::new(Irqs));
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);

    embassy_nrf::interrupt::RADIO.set_priority(embassy_nrf::interrupt::Priority::P0);
    embassy_nrf::interrupt::TIMER0.set_priority(embassy_nrf::interrupt::Priority::P0);
    embassy_nrf::interrupt::USBD.set_priority(embassy_nrf::interrupt::Priority::P2);
    embassy_nrf::interrupt::CLOCK_POWER.set_priority(embassy_nrf::interrupt::Priority::P2);

    spawner.spawn(logger_task(p.USBD)).unwrap();
    log::info!("Hello World!");
    let mut rad = TradRadio::new(p.RADIO, p.TIMER0, Irqs, Irqs, Addresses::default());
    rad.set_tx_addresses(|w| w.set_txaddress(0));
    rad.set_rx_addresses(|w| {
        w.set_addr1(true);
        w.set_addr2(true);
    });
    let mut packet = Packet::default();
    packet.copy_from_slice(&[0, 1, 2]);
    loop {
        let res = rad.send_packet(packet).await;
        log::info!(
            "Took {} us, {} retranmisisons",
            res.time_elapsed.as_micros(),
            res.retranmisisons
        );
        Timer::after_millis(1000).await;
    }
}
