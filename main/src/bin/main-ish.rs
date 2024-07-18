//! main.rs
//!
//! WIP: interfacing with the ST C library, both ways.

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;

use esp_hal::delay::Delay;

use embassy_executor::Spawner;
//use embassy_time::{Duration, Timer};
use esp_backtrace as _;

use esp_hal::{
    clock::ClockControl,
    peripherals::Peripherals,
    prelude::*,
    system::SystemControl,
    //timer::{timg::TimerGroup, ErasedTimer, OneShotTimer},
};

use vl53l5cx::VL53L5CX;

#[main]
async fn main(spawner: Spawner) {

    info!("Init!");
    let peripherals = Peripherals::take();
    let system = SystemControl::new(peripherals.SYSTEM);
    let clocks = ClockControl::boot_defaults(system.clock_control).freeze();

    info!("Let's do delay");
    let mut delay = Delay::new(&clocks);
    delay.delay_ms(1000_u32);

    let x = VL53L5CX::new();
    x.say();

    info!("Yee!");

    /***
    let timg0 = TimerGroup::new(peripherals.TIMG0, &clocks, None);
    let timer0 = OneShotTimer::new(timg0.timer0.into());
    let timers = [timer0];
    let timers = mk_static!([OneShotTimer<ErasedTimer>; 1], timers);
    esp_hal_embassy::init(&clocks, timers);

    loop {
        info!("Bing!");
        Timer::after(Duration::from_millis(5_000)).await;
    }***/
}