import asyncio
import math
import sys
import os

# Try to import from installed package, otherwise assume development environment
try:
    from BinaryOptionsToolsV2 import PocketOptionAsync
except ImportError:
    # Adjust path if running from examples/python/async
    sys.path.append(os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../BinaryOptionsToolsV2")))
    try:
        from BinaryOptionsToolsV2 import PocketOptionAsync
    except ImportError:
        print("BinaryOptionsToolsV2 not found. Please install it or check your path.")
        sys.exit(1)

# --- Matrix Math ---
class Matrix:
    def __init__(self, r, c, d=None):
        self.rows = r
        self.cols = c
        self.data = d if d else [0.0] * (r * c)

    @staticmethod
    def multiply(a, b):
        if a.cols != b.rows:
            raise ValueError("Shape Mismatch")
        res = Matrix(a.rows, b.cols)
        for i in range(a.rows):
            for j in range(b.cols):
                sum_val = 0.0
                for k in range(a.cols):
                    sum_val += a.data[i * a.cols + k] * b.data[k * b.cols + j]
                res.data[i * res.cols + j] = sum_val
        return res

    @staticmethod
    def add(a, b):
        if a.rows != b.rows or a.cols != b.cols:
             raise ValueError("Shape Mismatch in add")
        res = Matrix(a.rows, a.cols)
        for i in range(len(res.data)):
            res.data[i] = a.data[i] + b.data[i]
        return res

    def map(self, f):
        res = Matrix(self.rows, self.cols)
        for i in range(len(self.data)):
            res.data[i] = f(self.data[i])
        return res

# --- NanoNet Kernel ---
class NanoNet:
    def __init__(self):
        # Weights ported from AVIXTrades Neural Advisor v10.3

        # W1 promotes:
        # - Low RSI -> Hidden[0] (Oversold)
        # - High RSI -> Hidden[1] (Overbought)
        # - Pos Trend -> Hidden[2] (Bullish)
        # - Neg Trend -> Hidden[3] (Bearish)
        self.W1 = Matrix(5, 8, [
            -5.0,  5.0,  0.0,  0.0,  0.0,  0.0,  0.0,  0.0, # RSI impacts first 2
             0.0,  0.0,  2.0,  2.0,  0.0,  0.0,  0.0,  0.0, # Volatility
             0.0,  0.0,  5.0, -5.0,  0.0,  0.0,  0.0,  0.0, # Trend
             0.0,  0.0,  0.0,  0.0, -3.0,  0.0,  0.0,  0.0, # Shadows
             0.0,  0.0,  0.0,  0.0,  0.0, -3.0,  0.0,  0.0
        ])

        self.B1 = Matrix(1, 8, [2.5, -2.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])

        # W2 maps Hidden to Output [Hold, Call, Put]
        self.W2 = Matrix(8, 3, [
             0.0,  3.0, -1.0, # Oversold -> Call
             0.0, -1.0,  3.0, # Overbought -> Put
             0.0,  2.0, -2.0, # Bull Trend -> Call
             0.0, -2.0,  2.0, # Bear Trend -> Put
             1.0,  0.0,  0.0, # Noise
             1.0,  0.0,  0.0,
             0.0,  0.0,  0.0,
             0.0,  0.0,  0.0
        ])

        self.B2 = Matrix(1, 3, [1.0, 0.0, 0.0]) # Default bias towards Hold

    @staticmethod
    def tanh(x):
        return math.tanh(x)

    @staticmethod
    def softmax(arr):
        max_val = max(arr)
        exps = [math.exp(x - max_val) for x in arr]
        sum_exps = sum(exps)
        if sum_exps == 0:
            return [1.0/len(arr)] * len(arr)
        return [x / sum_exps for x in exps]

    def forward(self, inputs):
        X = Matrix(1, 5, inputs)
        Z1 = Matrix.add(Matrix.multiply(X, self.W1), self.B1)
        A1 = Z1.map(self.tanh)
        Z2 = Matrix.add(Matrix.multiply(A1, self.W2), self.B2)
        return self.softmax(Z2.data)

# --- Indicators ---
def calc_ema(d, p):
    if len(d) < 1:
        return 0.0
    if len(d) < p:
        return d[-1]
    k = 2.0 / (p + 1)
    ema = d[0]
    for i in range(1, len(d)):
        ema = d[i] * k + ema * (1 - k)
    return ema

def calc_rsi(d, p):
    if len(d) < p + 1:
        return 50.0
    gains = 0.0
    losses = 0.0
    for i in range(1, p + 1):
        df = d[i] - d[i - 1]
        if df >= 0:
            gains += df
        else:
            losses -= df
    ag = gains / p
    al = losses / p
    for i in range(p + 1, len(d)):
        df = d[i] - d[i - 1]
        if df >= 0:
            ag = (ag * (p - 1) + df) / p
            al = (al * (p - 1)) / p
        else:
            ag = (ag * (p - 1)) / p
            al = (al * (p - 1) - df) / p

    if al == 0:
        return 100.0
    return 100.0 - (100.0 / (1.0 + ag / al))

def calc_atr(candles, p):
    if len(candles) < p + 1:
        return 0.0
    s = 0.0
    for i in range(len(candles) - p, len(candles)):
        current = candles[i]
        prev = candles[i - 1]
        hl = current['h'] - current['l']
        hc = abs(current['h'] - prev['c'])
        lc = abs(current['l'] - prev['c'])
        s += max(hl, hc, lc)
    return s / p

# --- Main Logic ---
async def main():
    ssid = os.getenv("POCKETOPTION_SSID")
    if not ssid:
        print("Please set POCKETOPTION_SSID environment variable.")
        print("Example: export POCKETOPTION_SSID='your_ssid_here'")
        return

    print("Initializing AVIX Advisor (Neural Network V10.3 Port)...")
    client = PocketOptionAsync(ssid=ssid)

    print("Connecting to PocketOption...")
    await client.connect()

    asset = "EURUSD_otc"
    timeframe = 5 # 5 seconds
    print(f"Subscribing to {asset} ({timeframe}s candles)...")

    subscription = await client.subscribe_symbol(asset, timeframe)

    candles5s = []
    candles1m = []

    brain = NanoNet()

    print("Advisor Online. Collecting data...")

    async for candle in subscription:
        # candle structure from library usually has: time (sec), open, close, high, low
        c5 = {
            't': int(candle['time']) * 1000,
            'o': float(candle['open']),
            'h': float(candle['high']),
            'l': float(candle['low']),
            'c': float(candle['close'])
        }

        # Accumulate 5s candles
        # Note: If stream sends updates for the same candle, we must handle it.
        # Assuming unique candles for simplicity based on `subscription` iterator behavior.
        # But to be safe, we check timestamp.

        if not candles5s or candles5s[-1]['t'] != c5['t']:
            candles5s.append(c5)
            if len(candles5s) > 50:
                candles5s.pop(0)
        else:
            # Update existing candle
            candles5s[-1] = c5

        if len(candles5s) < 2:
            continue

        last5s = candles5s[-1]
        m1_time = (last5s['t'] // 60000) * 60000

        # 1m Candle Construction
        if not candles1m or candles1m[-1]['t'] != m1_time:
            candles1m.append({'t': m1_time, 'o': last5s['c'], 'h': last5s['c'], 'l': last5s['c'], 'c': last5s['c']})
            if len(candles1m) > 50:
                candles1m.pop(0)
        else:
            curr = candles1m[-1]
            curr['c'] = last5s['c']
            curr['h'] = max(curr['h'], last5s['c'])
            curr['l'] = min(curr['l'], last5s['c'])

        # Needs at least 15 candles for 14-period indicators to start making sense
        if len(candles5s) < 15:
            sys.stdout.write(f"\r collecting data... {len(candles5s)}/15")
            sys.stdout.flush()
            continue

        # Indicators
        close_prices = [c['c'] for c in candles5s]
        rsi_raw = calc_rsi(close_prices, 14)
        atr_raw = calc_atr(candles5s, 14)
        ema1m_prices = [c['c'] for c in candles1m]
        ema1m = calc_ema(ema1m_prices, 14)

        # Normalize
        n_rsi = rsi_raw / 100.0
        n_vol = (atr_raw / last5s['c']) * 1000.0 if last5s['c'] != 0 else 0
        if last5s['c'] != 0:
            n_trend = (last5s['c'] - ema1m) / last5s['c'] * 1000.0
        else:
            n_trend = 0

        n_high = (last5s['h'] - last5s['c']) / last5s['c'] * 1000.0 if last5s['c'] != 0 else 0
        n_low = (last5s['c'] - last5s['l']) / last5s['c'] * 1000.0 if last5s['c'] != 0 else 0

        input_vector = [n_rsi, n_vol, n_trend, n_high, n_low]
        probs = brain.forward(input_vector)

        p_hold, p_call, p_put = probs

        # Visual Output
        output = f"\rTime: {last5s['t']} | Price: {last5s['c']} | RSI: {rsi_raw:.2f} | C:{p_call*100:.0f}% P:{p_put*100:.0f}% H:{p_hold*100:.0f}%"

        # Logic for Signal
        if p_call > 0.7 and p_call > p_put:
             output += " \033[92m[CALL SIGNAL]\033[0m"
        elif p_put > 0.7 and p_put > p_call:
             output += " \033[91m[PUT SIGNAL]\033[0m"

        sys.stdout.write(output)
        sys.stdout.flush()

    await client.disconnect()

if __name__ == "__main__":
    asyncio.run(main())
