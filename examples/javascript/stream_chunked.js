const { PocketOption } = require("./binary-options-tools.node");

async function main(ssid) {
  // Initialize the API client
  const api = new PocketOption(ssid);

  // Wait for connection to establish
  await new Promise((resolve) => setTimeout(resolve, 5000));

  // Subscribe to a chunked symbol stream
  const stream = await api.subscribeSymbolChunked("EURUSD_otc");

  console.log("Starting chunked stream...");

  // Listen to the stream for 1 minute
  const endTime = Date.now() + 60000; // 60 seconds

  try {
    for await (const chunk of stream) {
      console.log("Received chunk:", chunk);
      console.log("Chunk timestamp:", new Date(chunk.timestamp).toISOString());
      console.log("Chunk data:", chunk.data);

      if (Date.now() > endTime) {
        console.log("Stream time finished");
        break;
      }
    }
  } catch (error) {
    console.error("Stream error:", error);
  } finally {
    // Clean up
    await stream.close();
  }
}

// Check if ssid is provided as command line argument
const ssid = "";

main(ssid).catch(console.error);
