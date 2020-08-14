/// This Connector obtains the nanoErg/USD rate and submits it
/// to an oracle core. It reads the `oracle-config.yaml` to find the port
/// of the oracle core (via Connector-Lib) and submits it to the POST API
/// server on the core.
/// Note: The value that is posted on-chain is the number
/// of nanoErgs per 1 USD, not the rate per nanoErg.
mod api;

use anyhow::{anyhow, Result};
use connector_lib::Connector;
use json;
use std::env;

static CONNECTOR_ASCII: &str = r#"
 ______ _____   _____        _    _  _____ _____     _____                            _
|  ____|  __ \ / ____|      | |  | |/ ____|  __ \   / ____|                          | |
| |__  | |__) | |  __ ______| |  | | (___ | |  | | | |     ___  _ __  _ __   ___  ___| |_ ___  _ __
|  __| |  _  /| | |_ |______| |  | |\___ \| |  | | | |    / _ \| '_ \| '_ \ / _ \/ __| __/ _ \| '__|
| |____| | \ \| |__| |      | |__| |____) | |__| | | |___| (_) | | | | | | |  __/ (__| || (_) | |
|______|_|  \_\\_____|       \____/|_____/|_____/   \_____\___/|_| |_|_| |_|\___|\___|\__\___/|_|
==================================================================================================
"#;

static CG_RATE_URL: &str =
    "https://api.coingecko.com/api/v3/simple/price?ids=ergo&vs_currencies=USD";

/// Acquires the nanoErg/USD price from CoinGecko
fn get_nanoerg_usd_price() -> Result<u64> {
    let resp = reqwest::blocking::Client::new().get(CG_RATE_URL).send()?;
    let price_json = json::parse(&resp.text()?)?;
    if let Some(p) = price_json["ergo"]["usd"].as_f64() {
        let nanoerg_price = (1.0 / p as f64) * 1000000000.0;
        return Ok(nanoerg_price as u64);
    } else {
        Err(anyhow!("Failed to parse price."))
    }
}

fn main() {
    // Check if asked for bootstrap value from connector
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && &args[1] == "--bootstrap-value" {
        if let Ok(price) = get_nanoerg_usd_price() {
            println!("Bootstrap Erg-USD Value: {}", price);
            std::process::exit(0);
        } else {
            panic!("Failed to fetch Erg/USD from CoinGecko");
        }
    }

    let connector = Connector::new_basic_connector(
        "ERG-USD",
        "Connector which fetches the number of nanoErgs per 1 USD.",
        get_nanoerg_usd_price,
    );
    connector.run();
}
