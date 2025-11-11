#![no_std]
#![no_main]

use core::sync::atomic::{AtomicBool, Ordering};

use bruh78::{
    key_config::set_keys,
    radio::{self, Addresses, Radio},
    sensors::DongleSensors,
};
use cortex_m_rt::entry;
use defmt::{info, *};
use embassy_executor::{Executor, InterruptExecutor};
use embassy_futures::join::{join, join3, join4};
use embassy_nrf::{
    bind_interrupts,
    config::HfclkSource,
    interrupt,
    interrupt::InterruptExt,
    peripherals::{self},
    qspi::Qspi,
    usb::{self, vbus_detect::HardwareVbusDetect, Driver},
    Peri,
};

use defmt_rtt as _; // global logger
use embassy_nrf as _;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::Timer;
use embassy_usb::{
    class::hid::{HidReaderWriter, HidWriter, State},
    Builder, Handler,
};
use key_lib::{
    com::Com,
    descriptor::{BufferReport, KeyboardReportNKRO, MouseReport},
    keys::{ConfigIndicator, Indicate, Keys},
    position::DefaultSwitch,
    report::Report,
    storage::Storage,
};
// time driver
use panic_probe as _;
use sequential_storage::cache::NoCache;
use static_cell::StaticCell;
use usbd_hid::descriptor::SerializedDescriptor;

static KEYS: Mutex<ThreadModeRawMutex, Keys<Indicator>> = Mutex::new(Keys::default());

static CACHE: StaticCell<NoCache> = StaticCell::new();

static RADIO_EXECUTOR: InterruptExecutor = InterruptExecutor::new();
static THREAD_EXECUTOR: StaticCell<Executor> = StaticCell::new();

bind_interrupts!(struct Irqs {
    USBD => usb::InterruptHandler<peripherals::USBD>;
    CLOCK_POWER => usb::vbus_detect::InterruptHandler;
    RADIO  => radio::InterruptHandler;
    // QSPI => embassy_nrf::qspi::InterruptHandler<peripherals::QSPI>;
});

#[embassy_executor::task]
async fn storage_task(storage: Storage<Qspi<'static, peripherals::QSPI>, NoCache>) {
    storage.run_storage().await;
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
    radio.run().await;
}

#[embassy_executor::task]
async fn thread_task(usbd: Peri<'static, peripherals::USBD>) {
    let driver = Driver::new(usbd, Irqs, HardwareVbusDetect::new(Irqs));

    // Create embassy-usb Config
    let mut config = embassy_usb::Config::new(0xa55, 0xa44);
    config.manufacturer = Some("Tybeast Corp.");
    config.product = Some("TyDongle");
    config.max_power = 500;
    config.max_packet_size_0 = 64;
    config.composite_with_iads = true;
    config.device_class = 0xef;
    config.device_sub_class = 0x02;
    config.device_protocol = 0x01;

    // Create embassy-usb DeviceBuilder using the driver and config.
    // It needs some buffers for building the descriptors.
    let mut config_descriptor = [0; 256];
    let mut bos_descriptor = [0; 256];
    let mut msos_descriptor = [0; 256];
    let mut control_buf = [0; 64];

    let mut key_state = State::new();
    let mut mouse_state = State::new();
    let mut com_state = State::new();
    let mut device_handler = MyDeviceHandler::new();

    let mut builder = Builder::new(
        driver,
        config,
        &mut config_descriptor,
        &mut bos_descriptor,
        &mut msos_descriptor,
        &mut control_buf,
    );

    // Create classes on the builder.
    let key_config = embassy_usb::class::hid::Config {
        report_descriptor: KeyboardReportNKRO::desc(),
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 32,
    };
    let com_config = embassy_usb::class::hid::Config {
        report_descriptor: BufferReport::desc(),
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 64,
    };
    let mouse_config = embassy_usb::class::hid::Config {
        report_descriptor: MouseReport::desc(),
        request_handler: None,
        poll_ms: 1,
        max_packet_size: 5,
    };
    builder.handler(&mut device_handler);
    let mut key_writer = HidWriter::<_, 32>::new(&mut builder, &mut key_state, key_config);
    let (com_reader, com_writer) =
        HidReaderWriter::<_, 32, 32>::new(&mut builder, &mut com_state, com_config).split();
    let mut mouse_writer = HidWriter::<_, 5>::new(&mut builder, &mut mouse_state, mouse_config);

    // Build the builder.
    let mut usb = builder.build();
    let usb_fut = usb.run();

    let sensors = DongleSensors::new();
    let mut report: Report<_, DefaultSwitch> = Report::new(sensors);

    let mut keys = KEYS.lock().await;
    set_keys(&mut keys);
    // keys.load_keys_from_storage(0).await;
    drop(keys);

    let mut com = Com::new(&KEYS, com_reader, com_writer);
    let key_loop = async {
        loop {
            let (key_rep, mouse_rep);
            {
                (key_rep, mouse_rep) = report.generate_report(&KEYS).await;
            }
            let key_task = async {
                if let Some(rep) = key_rep {
                    info!("Writing key report!");
                    key_writer.write_serialize(rep).await.unwrap();
                }
            };
            let mouse_task = async {
                if let Some(rep) = mouse_rep {
                    mouse_writer.write_serialize(rep).await.unwrap();
                }
            };
            join(key_task, mouse_task).await;
            Timer::after_micros(5).await;
        }
    };
    join3(usb_fut, key_loop, com.com_loop()).await;
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
        spawner.spawn(thread_task(p.USBD)).unwrap();
    });
}

struct Indicator {}

impl ConfigIndicator for Indicator {
    async fn indicate_config(&self, config_num: Indicate) {}
}

struct MyDeviceHandler {
    configured: AtomicBool,
}

impl MyDeviceHandler {
    fn new() -> Self {
        MyDeviceHandler {
            configured: AtomicBool::new(false),
        }
    }
}

impl Handler for MyDeviceHandler {
    fn enabled(&mut self, enabled: bool) {
        self.configured.store(false, Ordering::Relaxed);
        if enabled {
            info!("Device enabled");
        } else {
            info!("Device disabled");
        }
    }

    fn reset(&mut self) {
        self.configured.store(false, Ordering::Relaxed);
        info!("Bus reset, the Vbus current limit is 100mA");
    }

    fn addressed(&mut self, addr: u8) {
        self.configured.store(false, Ordering::Relaxed);
        info!("USB address set to: {}", addr);
    }

    fn configured(&mut self, configured: bool) {
        self.configured.store(configured, Ordering::Relaxed);
        if configured {
            info!(
                "Device configured, it may now draw up to the configured current limit from Vbus."
            )
        } else {
            info!("Device is no longer configured, the Vbus current limit is 100mA.");
        }
    }
}
