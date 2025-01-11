import asyncio
import websockets

# Target WebSocket server to proxy to
target_url = "wss://api.jp.stork-oracle.network/evm/subscribe"

# Function to handle incoming connections and forward them to the target server
async def proxy_handler(client_websocket, path):
    headers = dict(client_websocket.request_headers)

    # Connect to the target WebSocket server
    async with websockets.connect(target_url, extra_headers=headers) as target_websocket:
        try:
            # Relay messages between client and target server
            async def relay_messages(source, destination):
                async for message in source:
                    await destination.send(message)

            # Run both directions concurrently
            await asyncio.gather(
                relay_messages(client_websocket, target_websocket),
                relay_messages(target_websocket, client_websocket)
            )

        except websockets.exceptions.ConnectionClosedOK:
            print("Connection closed gracefully")

# Start the server
start_server = websockets.serve(proxy_handler, "localhost", 8765)

# Run the server
asyncio.get_event_loop().run_until_complete(start_server)
print("WebSocket proxy server started on ws://localhost:8765")
asyncio.get_event_loop().run_forever()