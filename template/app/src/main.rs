#![no_std]
#![no_main]

// Provides the Img2EntryFun0 bootloader entry table and VTOR pre_init.
use ameba_rt as _;

// cortex-m's critical-section-single-core implementation must be in the link.
use cortex_m as _;

use ameba_hal::println;

//IF option("embassy")
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

//IF option("wifi")
// Handles NP -> AP blocking API requests; must keep running while Wi-Fi is in use.
#[embassy_executor::task]
async fn whc_task() {
    ameba_wifi::run_blk_api().await;
}

//ENDIF
#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    ameba_hal::init(ameba_hal::Config::default());
    println!("{{project-name}} running");
//IF option("wifi")

    // Bring up Wi-Fi (STA mode). Connecting to an AP is left to you; see
    // examples/wifi-connect in the Rust SDK for wifi_connect, DHCP and TCP.
    ameba_wifi::init();
    _spawner.spawn(whc_task()).unwrap();
    let rc = ameba_wifi::wifi::wifi_on().await;
    println!("wifi_on: {}", rc);
//ENDIF
    loop {
        Timer::after(Duration::from_millis(1000)).await;
    }
}
//ELSE
use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    ameba_hal::init(ameba_hal::Config::default());
    println!("{{project-name}} running");
    loop {
        cortex_m::asm::wfi();
    }
}
//ENDIF
