#![no_std]
#![no_main]

use assign_resources::assign_resources;
use bruh78::radio::{self, Addresses, Packet, Radio};
use cortex_m_rt::entry;
use defmt::{info, *};
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_futures::join::join;
use embassy_nrf::{
    bind_interrupts,
    config::HfclkSource,
    gpio::{Input, Level, Output, OutputDrive, Pull},
    interrupt,
    interrupt::InterruptExt,
    peripherals::{self, USBD},
    usb::{self, vbus_detect::HardwareVbusDetect, Driver},
    Peri,
};

use defmt_rtt as _; // global logger
use embassy_nrf as _;
use embassy_time::Timer;
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

assign_resources! {
    keyboard: KeyboardResources {
        out_0: P1_00,
        out_1: P0_11,
        out_2: P1_04,
        out_3: P1_06,
        out_4: P0_09,
        in_0: P0_02,
        in_1: P1_15,
        in_2: P1_11,
        in_3: P0_10,
    },
    radio: RadioResources {
        rad: RADIO,
    }
    usbd: UsbdResources {
        usbd: USBD
    }
}

#[embassy_executor::task]
async fn logger_task(r: UsbdResources) {
    let driver = Driver::new(r.usbd, Irqs, HardwareVbusDetect::new(Irqs));
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
async fn radio_task(r: RadioResources) {
    let addresses = Addresses::default();
    let mut radio = Radio::new(r.rad, Irqs, addresses);
    radio.set_tx_addresses(|w| w.set_txaddress(1));
    radio.set_rx_addresses(|w| {
        w.set_addr0(true);
    });
    let mut packet = Packet::default();
    for i in 0..1000 {
        radio.receive(&mut packet).await;
        log::info!("Recevied packet {}", i);
    }
    loop {
        Timer::after_secs(1000).await;
    }
}

#[embassy_executor::task]
async fn thread_task(k: KeyboardResources) {
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
    log::info!("Hello World!");

    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);
    let r = split_resources!(p);

    embassy_nrf::interrupt::EGU1_SWI1.set_priority(embassy_nrf::interrupt::Priority::P1);
    embassy_nrf::interrupt::RADIO.set_priority(embassy_nrf::interrupt::Priority::P0);
    let spawner = RADIO_EXECUTOR.start(embassy_nrf::interrupt::EGU1_SWI1);
    spawner.spawn(radio_task(r.radio)).unwrap();

    let exectuor = THREAD_EXECUTOR.init_with(Executor::new);
    exectuor.run(|spawner| {
        spawner.spawn(logger_task(r.usbd)).unwrap();
        spawner.spawn(thread_task(r.keyboard)).unwrap();
    });
}
