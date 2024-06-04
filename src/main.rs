#![no_std]
#![no_main]

use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_net_wiznet::{chip::W5500, Device, Runner, State};
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::gpio::{Level, Output, Pull, Speed};
use embassy_stm32::i2c::{self, I2c};
// use embassy_stm32::interrupt;
use embassy_stm32::mode::Async;
use embassy_stm32::spi::{Config as SpiConfig, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::Config;
use embassy_time::Delay; //, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use static_cell::StaticCell;

use {defmt_rtt as _, panic_probe as _};

embassy_stm32::bind_interrupts!(struct Irqs {
    I2C1_EV => embassy_stm32::i2c::EventInterruptHandler<embassy_stm32::peripherals::I2C1>;
    I2C1_ER => embassy_stm32::i2c::ErrorInterruptHandler<embassy_stm32::peripherals::I2C1>;
});

// use embassy_executor::InterruptExecutor;
// static EXECUTOR_HIGH: InterruptExecutor = InterruptExecutor::new();
//
// use embassy_stm32::interrupt::InterruptExt;
// #[interrupt]
// unsafe fn USART6() {
//     EXECUTOR_HIGH.on_interrupt()
// }
//
// #[embassy_executor::task]
// async fn print_if_running_task() -> ! {
//     loop {
//         Timer::after_secs(1).await;
//         defmt::info!("still running");
//     }
// }

type EthernetSPI = ExclusiveDevice<Spi<'static, Async>, Output<'static>, Delay>;
#[embassy_executor::task]
async fn ethernet_task(
    runner: Runner<'static, W5500, EthernetSPI, ExtiInput<'static>, Output<'static>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<Device<'static>>) -> ! {
    stack.run().await
}

// 84 Mhz clock stm32f401
fn config() -> Config {
    use embassy_stm32::rcc::{
        AHBPrescaler, APBPrescaler, Hse, HseMode, Pll, PllMul, PllPDiv, PllPreDiv, PllSource,
        Sysclk,
    };

    let mut config = Config::default();
    config.rcc.hse = Some(Hse {
        freq: Hertz(25_000_000),
        mode: HseMode::Oscillator,
    });
    config.rcc.pll_src = PllSource::HSE;
    config.rcc.pll = Some(Pll {
        prediv: PllPreDiv::DIV25,
        mul: PllMul::MUL336,
        divp: Some(PllPDiv::DIV4),
        divq: None,
        divr: None,
    });
    config.rcc.ahb_pre = AHBPrescaler::DIV1;
    config.rcc.apb1_pre = APBPrescaler::DIV2;
    config.rcc.apb2_pre = APBPrescaler::DIV1;
    config.rcc.sys = Sysclk::PLL1_P;
    config
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_stm32::init(config());

    let mut i2c = I2c::new(
        p.I2C1,
        p.PB8,
        p.PB9,
        Irqs,
        p.DMA1_CH7,
        p.DMA1_CH0,
        // extra slow, helps with longer cable runs
        Hertz(150_000),
        i2c::Config::default(),
    );

    let mut spi_cfg = SpiConfig::default();
    spi_cfg.frequency = Hertz(50_000_000); // up to 50m works
    let (miso, mosi, clk) = (p.PA6, p.PA7, p.PA5);
    let spi = Spi::new(p.SPI1, clk, mosi, miso, p.DMA2_CH3, p.DMA2_CH0, spi_cfg);
    let cs = Output::new(p.PA4, Level::High, Speed::VeryHigh);
    let spi = unwrap!(ExclusiveDevice::new(spi, cs, Delay));

    let w5500_int = ExtiInput::new(p.PB0, p.EXTI0, Pull::Up);
    let w5500_reset = Output::new(p.PB1, Level::High, Speed::VeryHigh);

    let mac_addr = [0x02, 234, 3, 4, 82, 231];
    static STATE: StaticCell<State<8, 8>> = StaticCell::new();
    let state = STATE.init(State::<8, 8>::new());
    let (_, runner) = embassy_net_wiznet::new(mac_addr, state, spi, w5500_int, w5500_reset).await;
    unwrap!(spawner.spawn(ethernet_task(runner)));

    // embassy_stm32::interrupt::USART6.set_priority(embassy_stm32::interrupt::Priority::P6);
    // let spawner = EXECUTOR_HIGH.start(embassy_stm32::interrupt::USART6);
    // unwrap!(spawner.spawn(print_if_running_task()));

    info!("suspect write, might hang");
    unwrap!(i2c.write(0x77, &[0xe0, 0b10110110]).await);
    info!("write succeeded");

}
