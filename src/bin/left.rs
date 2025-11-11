//! This example test the RP Pico on board LED.
//!
//! It does not work with the RP Pico W board. See wifi_blinky.rs.

#![no_std]
#![no_main]

use assign_resources::assign_resources;
use bruh78::radio::{self, send_packet, Addresses, Packet, Radio};
use bruh78::sensors::Matrix;
use cortex_m_rt::entry;
use embassy_executor::{Executor, InterruptExecutor, Spawner};
use embassy_nrf::config::HfclkSource;
use embassy_nrf::gpio::{Input, Level, Output, OutputDrive, Pull};
use embassy_nrf::interrupt::InterruptExt;
use embassy_nrf::{bind_interrupts, interrupt, peripherals, Peri};
use embassy_time::Timer;
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

static RADIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();
static THREAD_EXECUTOR: StaticCell<Executor> = StaticCell::new();

bind_interrupts!(struct Irqs {
    RADIO => radio::InterruptHandler;
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
}

#[embassy_executor::task]
async fn radio_task(r: RadioResources) {
    let addresses = Addresses::default();
    let mut radio = Radio::new(r.rad, Irqs, addresses);
    radio.set_tx_addresses(|w| w.set_txaddress(1));
    radio.set_rx_addresses(|w| {
        w.set_addr0(true);
    });
    radio.run().await;
}

#[embassy_executor::task]
async fn keyboard_task(k: KeyboardResources) {
    let columns = [
        Output::new(k.out_0, Level::Low, OutputDrive::Standard),
        Output::new(k.out_1, Level::Low, OutputDrive::Standard),
        Output::new(k.out_2, Level::Low, OutputDrive::Standard),
        Output::new(k.out_3, Level::Low, OutputDrive::Standard),
        Output::new(k.out_4, Level::Low, OutputDrive::Standard),
    ];

    let rows = [
        Input::new(k.in_0, Pull::Down),
        Input::new(k.in_1, Pull::Down),
        Input::new(k.in_2, Pull::Down),
        Input::new(k.in_3, Pull::Down),
    ];

    let mut matrix = Matrix::new(columns, rows);
    matrix.disable_debouncer(15..17);
    let mut rep = 0;
    loop {
        matrix.update().await;
        let new_rep = matrix.get_state();
        if new_rep != rep {
            rep = new_rep;
            let mut packet = Packet::default();
            packet.copy_from_slice(&rep.to_le_bytes());
            send_packet(&packet).await;
        }
        Timer::after_micros(5).await;
    }
}

#[interrupt]
unsafe fn EGU1_SWI1() {
    RADIO_EXECUTOR.on_interrupt()
}

#[entry]
fn main() -> ! {
    let mut config = embassy_nrf::config::Config::default();
    config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(config);
    let r = split_resources!(p);

    embassy_nrf::interrupt::EGU1_SWI1.set_priority(embassy_nrf::interrupt::Priority::P1);
    embassy_nrf::interrupt::RADIO.set_priority(embassy_nrf::interrupt::Priority::P0);
    embassy_nrf::interrupt::GPIOTE.set_priority(embassy_nrf::interrupt::Priority::P2);
    let spawner = RADIO_EXECUTOR.start(embassy_nrf::interrupt::EGU1_SWI1);
    spawner.spawn(radio_task(r.radio)).unwrap();

    let executor = THREAD_EXECUTOR.init_with(Executor::new);
    executor.run(|spawner| {
        spawner.spawn(keyboard_task(r.keyboard)).unwrap();
    });
}
