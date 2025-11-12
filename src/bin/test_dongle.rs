#![no_std]
#![no_main]

use core::{mem, ops::Deref};

use bruh78::radio::{self, Addresses, LogInfo, Packet, Radio};
use cortex_m_rt::entry;
use defmt::{info, *};
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::join::join;
use embassy_nrf::{
    bind_interrupts,
    config::HfclkSource,
    gpio::Output,
    interrupt,
    interrupt::InterruptExt,
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

static RADIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();
static THREAD_EXECUTOR: StaticCell<Executor> = StaticCell::new();

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    RADIO  => radio::InterruptHandler;
});

#[embassy_executor::task]
async fn logger_task(usbd: Peri<'static, peripherals::USBD>) {
    let driver = Driver::new(usbd, Irqs, HardwareVbusDetect::new(Irqs));
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
async fn radio_task(radio: Peri<'static, peripherals::RADIO>) {
    let addresses = Addresses::default();
    let mut radio = Radio::new(radio, Irqs, addresses);
    radio.set_tx_addresses(|w| w.set_txaddress(0));
    radio.set_rx_addresses(|w| {
        w.set_addr1(true);
        w.set_addr2(true);
    });
    const N: usize = 1000;
    let mut log_state: Vec<LogInfo, N> = Vec::new();
    let mut packet = Packet::default();
    packet.copy_from_slice(&[1, 2, 3]);
    loop {
        let res = radio.send(&mut packet).await;
        log::info!(
            "Took {} us, {} retranmisisons",
            res.time_elapsed.as_micros(),
            res.retranmisisons
        );
        Timer::after_millis(1000).await;
    }
    // for _ in 0..N {
    //     let res = radio.send(&mut packet).await;
    //     log::info!("Sent one message!");
    //     log_state.push(res);
    //     Timer::after_millis(1).await;
    // }
    // for log in log_state {
    //     log::info!(
    //         "Duration Elapsed: {} | Number of retranmisisons: {}",
    //         log.time_elapsed.as_micros(),
    //         log.retranmisisons
    //     );
    // }
    // loop {
    //     Timer::after_secs(10000).await;
    // }
}

#[embassy_executor::task]
async fn thread_task() {
    loop {
        Timer::after_secs(1000).await;
    }
}

#[interrupt]
unsafe fn EGU1_SWI1() {
    RADIO_EXECUTOR.on_interrupt()
}

#[entry]
fn main() -> ! {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);

    embassy_nrf::interrupt::EGU1_SWI1.set_priority(embassy_nrf::interrupt::Priority::P1);
    embassy_nrf::interrupt::RADIO.set_priority(embassy_nrf::interrupt::Priority::P0);
    embassy_nrf::interrupt::USBD.set_priority(embassy_nrf::interrupt::Priority::P2);
    embassy_nrf::interrupt::CLOCK_POWER.set_priority(embassy_nrf::interrupt::Priority::P2);
    let spawner = RADIO_EXECUTOR.start(embassy_nrf::interrupt::EGU1_SWI1);
    spawner.spawn(radio_task(p.RADIO)).unwrap();

    let exectuor = THREAD_EXECUTOR.init_with(Executor::new);
    exectuor.run(|spawner| {
        spawner.spawn(logger_task(p.USBD)).unwrap();
        log::info!("Hello World!");
        spawner.spawn(thread_task()).unwrap();
    });
}
