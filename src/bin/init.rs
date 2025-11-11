#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_nrf::{
    bind_interrupts,
    config::HfclkSource,
    peripherals::{self, USBD},
    qspi::Qspi,
    usb::{self, vbus_detect::HardwareVbusDetect, Driver},
};

use defmt_rtt as _; // global logger
use embassy_nrf as _;
use embassy_time::Timer;
use key_lib::{
    codes::{ScanCodeBehavior, ScanCodeLayerStorage},
    storage::{self, get_item, store_val, Storage, StorageItem},
    NUM_KEYS,
};
// time driver
use panic_probe as _;
use sequential_storage::cache::NoCache;
use static_cell::StaticCell;

static CACHE: StaticCell<NoCache> = StaticCell::new();

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    QSPI => embassy_nrf::qspi::InterruptHandler<peripherals::QSPI>;
});

#[embassy_executor::task]
async fn logger_task(driver: Driver<'static, USBD, HardwareVbusDetect>) {
    embassy_usb_logger::run!(1024, log::LevelFilter::Info, driver);
}

#[embassy_executor::task]
async fn storage_task(storage: Storage<Qspi<'static, peripherals::QSPI>, NoCache>) {
    storage.run_storage().await;
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut nrf_config = embassy_nrf::config::Config::default();
    nrf_config.hfclk_source = HfclkSource::ExternalXtal;
    let p = embassy_nrf::init(nrf_config);

    let cache = CACHE.init_with(NoCache::new);
    let mut qspi_config = embassy_nrf::qspi::Config::default();
    qspi_config.sck_delay = 5;
    qspi_config.read_opcode = embassy_nrf::qspi::ReadOpcode::READ4O;
    qspi_config.write_opcode = embassy_nrf::qspi::WriteOpcode::PP4O;
    qspi_config.frequency = embassy_nrf::qspi::Frequency::M32;
    qspi_config.address_mode = embassy_nrf::qspi::AddressMode::_24BIT;
    qspi_config.capacity = 0x200000;

    let mut qspi_flash = Qspi::new(
        p.QSPI,
        Irqs,
        p.P0_21,
        p.P0_25,
        p.P0_20,
        p.P0_24,
        p.P0_22,
        p.P0_23,
        qspi_config,
    );
    // Enable quad operations
    // qspi_flash
    //     .custom_instruction(0x01, &[0u8, 0b10], &mut [])
    //     .await
    //     .unwrap();

    let driver = Driver::new(p.USBD, Irqs, HardwareVbusDetect::new(Irqs));
    spawner.spawn(logger_task(driver)).unwrap();

    let storage = Storage::init(qspi_flash, 0..(4096 * 5), cache).await;
    spawner.spawn(storage_task(storage)).unwrap();

    let key = storage::StorageKey::KeyScanCode {
        config_num: 0,
        layer: 0,
    };
    // let codes = ScanCodeLayerStorage {
    //     codes: [ScanCodeBehavior::Single(key_lib::scan_codes::KeyCodes::Undefined); NUM_KEYS],
    // };
    // store_val(key, &StorageItem::Key(codes)).await;
    let mut buffer = [0u8; 256];
    loop {
        let item = get_item(storage::StorageKey::KeyScanCode {
            config_num: 0,
            layer: 0,
        })
        .await;
        match item {
            Some(val) => {
                let StorageItem::Key(key) = val;
                log::info!("{:?}", key.codes);
            }
            None => {
                log::info!("No keys stored!???");
            }
        }

        Timer::after_secs(1).await;
    }
}
