const { PocketOption } = require("./binary-options-tools.node");
const { Validator } = require("./binary-options-tools.node");

async function main(ssid) {
  // Initialize the API client
  const api = new PocketOption(ssid);

  // Wait for connection to establish
  await new Promise((resolve) => setTimeout(resolve, 5000));

  // Basic raw order example
  try {
    const basicValidator = Validator.contains('"status":"success"');
    const basicResponse = await api.createRawOrder(
      '42["signals/subscribe"]',
      basicValidator,
    );
    console.log(`Basic raw order response: ${basicResponse}`);
  } catch (error) {
    console.log(`Basic raw order failed: ${error}`);
  }

  // Raw order with timeout example
  try {
    const timeoutValidator = Validator.regex(
      /{\"type\":\"signal\",\"data\":.*}/,
    );
    const timeoutResponse = await api.createRawOrderWithTimeout(
      '42["signals/load"]',
      timeoutValidator,
      { timeout: 5000 }, // 5 seconds
    );
    console.log(`Raw order with timeout response: ${timeoutResponse}`);
  } catch (error) {
    if (error.name === "TimeoutError") {
      console.log("Order timed out after 5 seconds");
    } else {
      console.log(`Order with timeout failed: ${error}`);
    }
  }

  // Raw order with timeout and retry example
  try {
    const retryValidator = Validator.all([
      Validator.contains('"type":"trade"'),
      Validator.contains('"status":"completed"'),
    ]);

    const retryResponse = await api.createRawOrderWithTimeoutAndRetry(
      '42["trade/subscribe"]',
      retryValidator,
      { timeout: 10000 }, // 10 seconds
    );
    console.log(`Raw order with retry response: ${retryResponse}`);
  } catch (error) {
    console.log(`Order with retry failed: ${error}`);
  }
}

// Check if ssid is provided as command line argument
const ssid = "";

main(ssid).catch(console.error);
