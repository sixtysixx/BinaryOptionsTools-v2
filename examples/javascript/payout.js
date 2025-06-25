const { PocketOption } = require("./binary-options-tools.node");

async function main(ssid) {
  // Initialize the API client
  const api = new PocketOption(ssid);

  // Wait for connection to establish
  await new Promise((resolve) => setTimeout(resolve, 5000));

  // Get payout for all assets
  const payouts = await api.payout();
  console.log("All payouts:", payouts);

  // Get payout for specific asset
  const eurUsdPayout = await api.payout("EURUSD_otc");
  console.log("EUR/USD payout:", eurUsdPayout);

  // Get multiple specific payouts
  const assets = ["EURUSD_otc", "GBPUSD_otc", "USDJPY_otc"];
  const specificPayouts = await Promise.all(
    assets.map((asset) => api.payout(asset)),
  );

  assets.forEach((asset, index) => {
    console.log(`${asset} payout:`, specificPayouts[index]);
  });
}

// Check if ssid is provided as command line argument
const ssid = "";

main(ssid).catch(console.error);
