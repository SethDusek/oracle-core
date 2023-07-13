use std::pin::Pin;

use futures::Future;

use super::{
    assets_exchange_rate::{convert, AssetsExchangeRate, NanoErg, Btc},
    coincap, coingecko, DataPointSourceError,
};

#[allow(clippy::type_complexity)]
pub fn nanoerg_btc_sources() -> Vec<
    Pin<Box<dyn Future<Output = Result<AssetsExchangeRate<Btc, NanoErg>, DataPointSourceError>>>>,
> {
    vec![
        Box::pin(coingecko::get_btc_nanoerg()),
        Box::pin(get_btc_nanoerg_coincap()),
    ]
}

// Calculate ERG/BTC through ERG/USD and USD/BTC
async fn get_btc_nanoerg_coincap() -> Result<AssetsExchangeRate<Btc, NanoErg>, DataPointSourceError>
{
    Ok(convert(
        coincap::get_usd_nanoerg().await?,
        coincap::get_btc_usd().await?,
    ))
}
