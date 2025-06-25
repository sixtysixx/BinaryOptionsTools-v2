const { PocketOption } = require("./binary-options-tools.node");
const { Validator } = require("./binary-options-tools.node");

async function main(ssid) {
  // Initialize the API client
  const api = new PocketOption(ssid);

  // Wait for connection to establish
  await new Promise((resolve) => setTimeout(resolve, 5000));

  // Create a validator for price updates
  const validator = Validator.regex(/{"price":\d+\.\d+}/);

  try {
    // Create an iterator with 5 minute timeout
    const stream = api.createRawIterator(
      '42["price/subscribe"]', // WebSocket subscription message
      validator,
      { timeout: 5 * 60 * 1000 }, // 5 minutes in milliseconds
    );

    // Process messages as they arrive
    for await (const message of stream) {
      console.log(`Received message: ${message}`);
    }
  } catch (error) {
    if (error.name === "TimeoutError") {
      console.log("Stream timed out after 5 minutes");
    } else {
      console.log(`Error processing stream: ${error}`);
    }
  }
}

const ssid = "";

main(ssid).catch(console.error);
