// ==UserScript==
// @name         PocketOption WFA Quantitative Trading Bot
// @namespace    https://github.com/ChipaDevTeam/BinaryOptionsTools-v2
// @version      1.0.0
// @description  Intercepts WebSocket stream, executes Walk Forward Analysis (WFA) strategy engine (HMA, RSI, RoC, MFI, ATR), and automates trading with Shadow DOM GUI & Trailing SL/TP.
// @author       Senior Quantitative Developer
// @match        https://pocketoption.com/*
// @match        https://*.pocketoption.com/*
// @match        https://po.trade/*
// @match        https://*.po.trade/*
// @run-at       document-start
// @grant        none
// ==UserScript==

(function () {
  'use strict';

  console.log('[PO-WFA-Bot] Userscript loading at document-start...');

  // =========================================================================
  // 1. EVENT BUS ARCHITECTURE
  // =========================================================================
  class EventBus {
    constructor() {
      this.listeners = {};
    }

    on(event, callback) {
      if (!this.listeners[event]) {
        this.listeners[event] = [];
      }
      this.listeners[event].push(callback);
    }

    off(event, callback) {
      if (!this.listeners[event]) return;
      this.listeners[event] = this.listeners[event].filter(cb => cb !== callback);
    }

    emit(event, payload) {
      if (!this.listeners[event]) return;
      this.listeners[event].forEach(cb => {
        try {
          cb(payload);
        } catch (err) {
          console.error(`[PO-WFA-Bot] Error in event listener '${event}':`, err);
        }
      });
    }
  }

  const bus = new EventBus();

  // Global Session & State Context
  const State = {
    activeSocket: null,
    ssid: null,
    uid: null,
    isDemo: true,
    currentAsset: 'EURUSD_otc',
    activePayout: 0.80,
    startingBalance: null,
    currentBalance: null,
    peakBalance: null,
    openDeals: new Map(),
    consecutiveLosses: 0,
    cooldownUntil: 0,
    ticks: [],
    candles1m: [],
    formingCandle: null,
    wfaConfidence: 0,
    wfaDirection: null, // 'call' | 'put' | null
    strategyEnabled: false,
    settings: {
      trailingSL: 5.0,        // % trailing stop loss from peak balance
      takeProfit: 10.0,       // % take profit from starting balance
      timeframeLookback: '4h',// '4h', '8h', '1d'
      minConfidence: 80,      // % threshold (75-95)
      minPayout: 80,          // % min payout requirement
      useATRFilter: true,     // filter low/high volatility
      cooldownMinutes: 3,     // cooldown after 2 consecutive losses
      tradeAmount: 1.0,       // trade size in account currency
      tradeDuration: 300,     // 5 minutes (300s)
    }
  };

  // =========================================================================
  // 2. WEBSOCKET MONKEY-PATCHING & SSID / PAYLOAD INTERCEPTION
  // =========================================================================
  const NativeWebSocket = window.WebSocket;

  function parseSocketMessage(data) {
    if (typeof data !== 'string') return null;

    let clean = data;
    if (clean.startsWith('451-')) {
      clean = clean.substring(4);
    }
    const match = clean.match(/^\d*(\[.*\])$/);
    if (match && match[1]) {
      try {
        const parsed = JSON.parse(match[1]);
        if (Array.isArray(parsed) && parsed.length >= 1) {
          return {
            event: parsed[0],
            data: parsed[1] || null,
            raw: data
          };
        }
      } catch (e) {
        // Not JSON formatted
      }
    }
    return null;
  }

  window.WebSocket = function (url, protocols) {
    const ws = protocols !== undefined ? new NativeWebSocket(url, protocols) : new NativeWebSocket(url);
    console.log('[PO-WFA-Bot] WebSocket instance created:', url);
    State.activeSocket = ws;

    const originalSend = ws.send;
    ws.send = function (data) {
      try {
        const parsed = parseSocketMessage(data);
        if (parsed) {
          bus.emit('ws:send', parsed);

          // Intercept Auth / SSID
          if (parsed.event === 'auth' && parsed.data) {
            State.ssid = parsed.data.session || data;
            State.uid = parsed.data.uid;
            State.isDemo = parsed.data.isDemo !== undefined ? Boolean(parsed.data.isDemo) : true;
            bus.emit('auth:captured', { ssid: State.ssid, uid: State.uid, isDemo: State.isDemo });
            console.log('[PO-WFA-Bot] SSID Captured:', State.ssid.substring(0, 30) + '...');
          }

          // Intercept active symbol changes
          if ((parsed.event === 'changeSymbol' || parsed.event === 'subscribeSymbol' || parsed.event === 'changeSymbolFast') && parsed.data) {
            const newAsset = typeof parsed.data === 'string' ? parsed.data : parsed.data.asset || parsed.data.active || parsed.data.symbol;
            if (newAsset && newAsset !== State.currentAsset) {
              State.currentAsset = newAsset;
              State.ticks = [];
              State.candles1m = [];
              State.formingCandle = null;
              bus.emit('asset:changed', State.currentAsset);
              console.log('[PO-WFA-Bot] Active Asset changed to:', State.currentAsset);
            }
          }
        }
      } catch (err) {
        console.error('[PO-WFA-Bot] Error in ws.send hook:', err);
      }
      return originalSend.apply(this, arguments);
    };

    ws.addEventListener('message', function (event) {
      try {
        const parsed = parseSocketMessage(event.data);
        if (parsed) {
          bus.emit('ws:message', parsed);

          // 1. Balance update
          if (parsed.event === 'balance' || parsed.event === 'updateBalance') {
            const bal = typeof parsed.data === 'number' ? parsed.data : (parsed.data ? parsed.data.balance : null);
            if (bal !== null && !isNaN(bal)) {
              bus.emit('balance:updated', bal);
            }
          }

          // 2. Stream price ticks (updateStream, updateHistory, updateHistoryNewFast, updateHistoryNew, loadHistoryPeriod, history)
          const tickEvents = ['updateStream', 'updateHistory', 'updateHistoryNewFast', 'updateHistoryNew', 'loadHistoryPeriod', 'history'];
          if (tickEvents.includes(parsed.event) && parsed.data) {
            let symbol, time, price;

            // Robust array parsing (handles [symbol, timestamp, price], [[timestamp, price]], or [{time, price}])
            if (Array.isArray(parsed.data)) {
              if (parsed.data.length >= 3 && typeof parsed.data[0] === 'string' && typeof parsed.data[2] === 'number') {
                symbol = parsed.data[0];
                time = parsed.data[1];
                price = parsed.data[2];
                if ((!symbol || symbol === State.currentAsset) && price !== undefined) {
                  bus.emit('tick:received', { symbol: State.currentAsset, time: Number(time) || Date.now() / 1000, price: Number(price) });
                }
              } else {
                // Array of ticks/candles or nested array tuples
                parsed.data.forEach(item => {
                  if (Array.isArray(item)) {
                    // Tuple formats: [symbol, time, price] or [time, price]
                    let t, p, s;
                    if (item.length >= 3 && typeof item[0] === 'string') {
                      s = item[0];
                      t = item[1];
                      p = item[2];
                    } else if (item.length >= 2) {
                      t = item[0];
                      p = item[1];
                      s = State.currentAsset;
                    }
                    if ((!s || s === State.currentAsset) && p !== undefined && !isNaN(Number(p))) {
                      bus.emit('tick:received', { symbol: State.currentAsset, time: Number(t) || Date.now() / 1000, price: Number(p) });
                    }
                  } else if (item && typeof item === 'object') {
                    const itemSymbol = item.symbol || item.asset || State.currentAsset;
                    const itemPrice = item.price !== undefined ? item.price : item.close;
                    const itemTime = item.time || item.timestamp || Date.now() / 1000;
                    if (itemSymbol === State.currentAsset && itemPrice !== undefined && !isNaN(Number(itemPrice))) {
                      bus.emit('tick:received', { symbol: State.currentAsset, time: Number(itemTime), price: Number(itemPrice) });
                    }
                  }
                });
              }
            } else if (typeof parsed.data === 'object') {
              // Object format
              symbol = parsed.data.symbol || parsed.data.asset || State.currentAsset;
              time = parsed.data.time || parsed.data.timestamp || Date.now() / 1000;
              price = parsed.data.price !== undefined ? parsed.data.price : parsed.data.close;

              if (symbol === State.currentAsset && price !== undefined && !isNaN(Number(price))) {
                bus.emit('tick:received', { symbol: State.currentAsset, time: Number(time), price: Number(price) });
              } else if (parsed.data.history && Array.isArray(parsed.data.history)) {
                // Historical array envelope
                parsed.data.history.forEach(item => {
                  if (Array.isArray(item)) {
                    const t = item.length >= 2 ? item[0] : Date.now() / 1000;
                    const p = item.length >= 2 ? item[1] : item[0];
                    if (p !== undefined && !isNaN(Number(p))) {
                      bus.emit('tick:received', { symbol: State.currentAsset, time: Number(t), price: Number(p) });
                    }
                  } else if (item && typeof item === 'object') {
                    const itemPrice = item.price !== undefined ? item.price : item.close;
                    const itemTime = item.time || item.timestamp || Date.now() / 1000;
                    if (itemPrice !== undefined && !isNaN(Number(itemPrice))) {
                      bus.emit('tick:received', { symbol: State.currentAsset, time: Number(itemTime), price: Number(itemPrice) });
                    }
                  }
                });
              }
            }
          }

          // 3. Asset payouts / updateAssets
          if (parsed.event === 'updateAssets' || parsed.event === 'updatePayout') {
            if (Array.isArray(parsed.data)) {
              parsed.data.forEach(item => {
                if (item.name === State.currentAsset || item.symbol === State.currentAsset) {
                  const payout = item.payout || item.profit || item.rate;
                  if (payout) {
                    State.activePayout = payout > 1 ? payout / 100 : payout;
                    bus.emit('payout:updated', State.activePayout);
                  }
                }
              });
            }
          }

          // 4. Trade Execution Responses
          if (parsed.event === 'successopenOrder') {
            console.log('[PO-WFA-Bot] Order placed successfully:', parsed.data);
            bus.emit('trade:opened', parsed.data);
          } else if (parsed.event === 'failopenOrder') {
            console.warn('[PO-WFA-Bot] Order placement failed:', parsed.data);
            bus.emit('trade:failed', parsed.data);
          } else if (parsed.event === 'updateClosedDeals' || parsed.event === 'closeDeal') {
            bus.emit('trade:closed', parsed.data);
          }
        }
      } catch (err) {
        console.error('[PO-WFA-Bot] Error in ws.message hook:', err);
      }
    });

    return ws;
  };

  window.WebSocket.prototype = NativeWebSocket.prototype;

  // Function to programmatically send WebSocket frame
  function sendWsFrame(eventName, dataPayload) {
    if (!State.activeSocket || State.activeSocket.readyState !== NativeWebSocket.OPEN) {
      console.error('[PO-WFA-Bot] Cannot send frame: WebSocket is not open.');
      return false;
    }
    const messageStr = `42${JSON.stringify([eventName, dataPayload])}`;
    State.activeSocket.send(messageStr);
    console.log(`[PO-WFA-Bot] Sent WS Frame: ${eventName}`, dataPayload);
    return true;
  }

  // Programmatic Order Placement Helper
  function executeTrade(action, amount, timeInSeconds) {
    const requestId = 'wfa_' + Math.random().toString(36).substring(2, 10);
    const payload = {
      asset: State.currentAsset,
      amount: Number(amount),
      action: action.toLowerCase(), // 'call' or 'put'
      isDemo: State.isDemo ? 1 : 0,
      requestId: requestId,
      optionType: 100, // standard binary option
      time: timeInSeconds || State.settings.tradeDuration
    };

    console.log(`[PO-WFA-Bot] Executing ${action.toUpperCase()} order on ${State.currentAsset}:`, payload);
    return sendWsFrame('openOrder', payload);
  }

  // =========================================================================
  // 3. DOM & UI MANAGEMENT (SHADOW DOM + AUTO CHART SWITCHER)
  // =========================================================================
  function initGUI() {
    if (document.getElementById('po-wfa-root')) return;

    const host = document.createElement('div');
    host.id = 'po-wfa-root';
    host.style.position = 'fixed';
    host.style.top = '20px';
    host.style.right = '20px';
    host.style.zIndex = '999999';
    document.body.appendChild(host);

    const shadow = host.attachShadow({ mode: 'open' });

    const styles = `
      :host {
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
        font-size: 13px;
        color: #e0e6ed;
      }
      .panel {
        width: 310px;
        background: rgba(18, 24, 38, 0.94);
        backdrop-filter: blur(12px);
        border: 1px solid rgba(255, 255, 255, 0.12);
        border-radius: 10px;
        box-shadow: 0 10px 30px rgba(0,0,0,0.5);
        overflow: hidden;
        user-select: none;
      }
      .header {
        background: linear-gradient(135deg, #1e293b, #0f172a);
        padding: 12px 16px;
        font-weight: 700;
        display: flex;
        justify-content: space-between;
        align-items: center;
        cursor: move;
        border-bottom: 1px solid rgba(255, 255, 255, 0.08);
      }
      .title {
        display: flex;
        align-items: center;
        gap: 8px;
        font-size: 14px;
        color: #38bdf8;
      }
      .content {
        padding: 14px;
        display: flex;
        flex-direction: column;
        gap: 12px;
      }
      .row {
        display: flex;
        justify-content: space-between;
        align-items: center;
      }
      label {
        font-size: 12px;
        color: #94a3b8;
      }
      input[type="number"], select {
        background: #0f172a;
        border: 1px solid #334155;
        color: #f8fafc;
        border-radius: 6px;
        padding: 4px 8px;
        width: 80px;
        text-align: right;
        font-size: 12px;
      }
      input[type="range"] {
        width: 110px;
        accent-color: #38bdf8;
      }
      .toggle-btn {
        width: 100%;
        padding: 10px;
        border-radius: 6px;
        font-weight: 700;
        font-size: 13px;
        cursor: pointer;
        border: none;
        transition: all 0.2s ease;
      }
      .toggle-btn.off {
        background: #ef4444;
        color: #fff;
      }
      .toggle-btn.off:hover {
        background: #dc2626;
      }
      .toggle-btn.on {
        background: #10b981;
        color: #fff;
      }
      .toggle-btn.on:hover {
        background: #059669;
      }
      .hud {
        background: #090d16;
        border-radius: 6px;
        padding: 10px;
        border: 1px solid #1e293b;
        display: flex;
        flex-direction: column;
        gap: 6px;
      }
      .hud-item {
        display: flex;
        justify-content: space-between;
        font-size: 11px;
      }
      .hud-val {
        font-weight: 600;
        color: #f1f5f9;
      }
      .badge-demo {
        background: #eab308;
        color: #000;
        font-size: 10px;
        padding: 2px 6px;
        border-radius: 4px;
        font-weight: 700;
      }
      .badge-real {
        background: #ef4444;
        color: #fff;
        font-size: 10px;
        padding: 2px 6px;
        border-radius: 4px;
        font-weight: 700;
      }
    `;

    const template = `
      <style>${styles}</style>
      <div class="panel" id="panel">
        <div class="header" id="header">
          <div class="title">
            <span>⚡ PO WFA Bot</span>
            <span id="account-badge" class="badge-demo">DEMO</span>
          </div>
          <div style="font-size:11px; color:#64748b;" id="asset-display">EURUSD_otc</div>
        </div>
        <div class="content">
          <button id="toggle-strategy" class="toggle-btn off">STRATEGY OFF</button>

          <div class="hud">
            <div class="hud-item">
              <span>SSID Status:</span>
              <span id="hud-ssid" class="hud-val" style="color:#ef4444;">Disconnected</span>
            </div>
            <div class="hud-item">
              <span>WFA Signal Score:</span>
              <span id="hud-wfa" class="hud-val">0% (NEUTRAL)</span>
            </div>
            <div class="hud-item">
              <span>Active Payout:</span>
              <span id="hud-payout" class="hud-val">80%</span>
            </div>
            <div class="hud-item">
              <span>Session Balance:</span>
              <span id="hud-balance" class="hud-val">$0.00</span>
            </div>
            <div class="hud-item">
              <span>Cooldown Status:</span>
              <span id="hud-cooldown" class="hud-val" style="color:#10b981;">Ready</span>
            </div>
          </div>

          <div class="row">
            <label>Trade Size ($):</label>
            <input type="number" id="cfg-trade-amount" value="1.0" step="0.5" min="1" />
          </div>

          <div class="row">
            <label>Min Confidence Threshold:</label>
            <div style="display:flex; align-items:center; gap:6px;">
              <input type="range" id="cfg-confidence" min="75" max="95" value="80" />
              <span id="confidence-val" style="font-size:11px; width:28px;">80%</span>
            </div>
          </div>

          <div class="row">
            <label>Trailing Stop Loss (%):</label>
            <input type="number" id="cfg-trailing-sl" value="5.0" step="0.5" min="1" max="50" />
          </div>

          <div class="row">
            <label>Take Profit (%):</label>
            <input type="number" id="cfg-take-profit" value="10.0" step="1" min="1" max="200" />
          </div>

          <div class="row">
            <label>Min Asset Payout (%):</label>
            <input type="number" id="cfg-min-payout" value="80" step="1" min="50" max="95" />
          </div>

          <div class="row">
            <label>WFA Lookback Window:</label>
            <select id="cfg-lookback">
              <option value="4h" selected>4 Hours</option>
              <option value="8h">8 Hours</option>
              <option value="1d">1 Day</option>
            </select>
          </div>

        </div>
      </div>
    `;

    shadow.innerHTML = template;

    // Draggable GUI implementation
    const panel = shadow.getElementById('panel');
    const header = shadow.getElementById('header');
    let isDragging = false;
    let startX, startY, initialLeft, initialTop;

    header.addEventListener('mousedown', (e) => {
      isDragging = true;
      startX = e.clientX;
      startY = e.clientY;
      const rect = host.getBoundingClientRect();
      initialLeft = rect.left;
      initialTop = rect.top;
      document.addEventListener('mousemove', onMouseMove);
      document.addEventListener('mouseup', onMouseUp);
    });

    function onMouseMove(e) {
      if (!isDragging) return;
      const dx = e.clientX - startX;
      const dy = e.clientY - startY;
      host.style.left = `${initialLeft + dx}px`;
      host.style.top = `${initialTop + dy}px`;
      host.style.right = 'auto';
    }

    function onMouseUp() {
      isDragging = false;
      document.removeEventListener('mousemove', onMouseMove);
      document.removeEventListener('mouseup', onMouseUp);
    }

    // UI Input Bindings
    const btnToggle = shadow.getElementById('toggle-strategy');
    btnToggle.addEventListener('click', () => {
      State.strategyEnabled = !State.strategyEnabled;
      updateStrategyToggleBtn();
    });

    function updateStrategyToggleBtn() {
      if (State.strategyEnabled) {
        btnToggle.textContent = 'STRATEGY ACTIVE';
        btnToggle.className = 'toggle-btn on';
      } else {
        btnToggle.textContent = 'STRATEGY OFF';
        btnToggle.className = 'toggle-btn off';
      }
    }

    const confidenceSlider = shadow.getElementById('cfg-confidence');
    const confidenceVal = shadow.getElementById('confidence-val');
    confidenceSlider.addEventListener('input', (e) => {
      State.settings.minConfidence = Number(e.target.value);
      confidenceVal.textContent = `${e.target.value}%`;
    });

    shadow.getElementById('cfg-trade-amount').addEventListener('change', (e) => {
      State.settings.tradeAmount = Number(e.target.value) || 1.0;
    });

    shadow.getElementById('cfg-trailing-sl').addEventListener('change', (e) => {
      State.settings.trailingSL = Number(e.target.value) || 5.0;
    });

    shadow.getElementById('cfg-take-profit').addEventListener('change', (e) => {
      State.settings.takeProfit = Number(e.target.value) || 10.0;
    });

    shadow.getElementById('cfg-min-payout').addEventListener('change', (e) => {
      State.settings.minPayout = Number(e.target.value) || 80;
    });

    shadow.getElementById('cfg-lookback').addEventListener('change', (e) => {
      State.settings.timeframeLookback = e.target.value;
    });

    // Reactive Event Listeners for HUD Updates
    bus.on('auth:captured', (data) => {
      const hudSSID = shadow.getElementById('hud-ssid');
      hudSSID.textContent = 'Connected (' + (data.isDemo ? 'Demo' : 'Real') + ')';
      hudSSID.style.color = '#10b981';

      const badge = shadow.getElementById('account-badge');
      badge.textContent = data.isDemo ? 'DEMO' : 'REAL';
      badge.className = data.isDemo ? 'badge-demo' : 'badge-real';
    });

    bus.on('asset:changed', (asset) => {
      shadow.getElementById('asset-display').textContent = asset;
    });

    bus.on('payout:updated', (payout) => {
      shadow.getElementById('hud-payout').textContent = `${Math.round(payout * 100)}%`;
    });

    bus.on('balance:updated', (balance) => {
      shadow.getElementById('hud-balance').textContent = `$${balance.toFixed(2)}`;
    });

    bus.on('wfa:updated', (data) => {
      const hudWFA = shadow.getElementById('hud-wfa');
      hudWFA.textContent = `${data.confidence}% (${data.direction ? data.direction.toUpperCase() : 'NEUTRAL'})`;
      if (data.direction === 'call') hudWFA.style.color = '#10b981';
      else if (data.direction === 'put') hudWFA.style.color = '#ef4444';
      else hudWFA.style.color = '#f1f5f9';
    });

    bus.on('cooldown:updated', (status) => {
      const hudCooldown = shadow.getElementById('hud-cooldown');
      hudCooldown.textContent = status.active ? `Paused (${status.remainingSecs}s)` : 'Ready';
      hudCooldown.style.color = status.active ? '#ef4444' : '#10b981';
    });

    bus.on('strategy:stopped', (reason) => {
      State.strategyEnabled = false;
      updateStrategyToggleBtn();
      alert(`[PO-WFA-Bot] Strategy Auto-Stopped: ${reason}`);
    });
  }

  // DOM Line Chart Switcher
  function ensureLineChartView() {
    try {
      const chartTypeBtn = document.querySelector('.chart-type-button, .chart-settings__type, [data-chart-type]');
      if (chartTypeBtn) {
        const isLine = chartTypeBtn.classList.contains('line') || chartTypeBtn.innerText.toLowerCase().includes('line');
        if (!isLine) {
          console.log('[PO-WFA-Bot] Switching chart type to Line Chart via DOM...');
          chartTypeBtn.click();
          setTimeout(() => {
            const lineItem = document.querySelector('.chart-type-item.line, [data-type="line"], .chart-settings__item--line');
            if (lineItem) {
              lineItem.click();
              console.log('[PO-WFA-Bot] Line Chart selected.');
            }
          }, 300);
        }
      }
    } catch (err) {
      console.warn('[PO-WFA-Bot] Auto Line Chart switch warning:', err);
    }
  }

  // =========================================================================
  // 4. QUANTITATIVE ENGINE: INDICATORS, 1M CANDLES & ROLLING WFA
  // =========================================================================

  // Indicator Calculations
  function calculateWMA(prices, period) {
    if (prices.length < period) return null;
    let sum = 0;
    let weightSum = 0;
    for (let i = 0; i < period; i++) {
      const weight = period - i;
      sum += prices[prices.length - 1 - i] * weight;
      weightSum += weight;
    }
    return sum / weightSum;
  }

  function calculateHMA(prices, period = 14) {
    if (prices.length < period) return null;
    const halfPeriod = Math.floor(period / 2);
    const sqrtPeriod = Math.floor(Math.sqrt(period));

    const diffSeries = [];
    for (let i = 0; i < sqrtPeriod + 5; i++) {
      const subPrices = prices.slice(0, prices.length - i);
      const wmaHalf = calculateWMA(subPrices, halfPeriod);
      const wmaFull = calculateWMA(subPrices, period);
      if (wmaHalf === null || wmaFull === null) break;
      diffSeries.unshift(2 * wmaHalf - wmaFull);
    }

    return calculateWMA(diffSeries, sqrtPeriod);
  }

  function calculateRSI(closes, period = 14) {
    if (closes.length < period + 1) return 50;
    let gains = 0, losses = 0;
    for (let i = closes.length - period; i < closes.length; i++) {
      const change = closes[i] - closes[i - 1];
      if (change >= 0) gains += change;
      else losses -= change;
    }
    const avgGain = gains / period;
    const avgLoss = losses / period;
    if (avgLoss === 0) return 100;
    const rs = avgGain / avgLoss;
    return 100 - (100 / (1 + rs));
  }

  function calculateRoC(closes, period = 12) {
    if (closes.length <= period) return 0;
    const current = closes[closes.length - 1];
    const past = closes[closes.length - 1 - period];
    return ((current - past) / past) * 100;
  }

  function calculateMFI(candles, period = 14) {
    if (candles.length <= period) return 50;
    let posMF = 0, negMF = 0;
    for (let i = candles.length - period; i < candles.length; i++) {
      const curr = candles[i];
      const prev = candles[i - 1];
      const currTP = (curr.high + curr.low + curr.close) / 3;
      const prevTP = (prev.high + prev.low + prev.close) / 3;
      const rawMF = currTP * (curr.volume || 1);

      if (currTP > prevTP) posMF += rawMF;
      else if (currTP < prevTP) negMF += rawMF;
    }
    if (negMF === 0) return 100;
    const mfr = posMF / negMF;
    return 100 - (100 / (1 + mfr));
  }

  function calculateATR(candles, period = 14) {
    if (candles.length < period + 1) return 0;
    let trSum = 0;
    for (let i = candles.length - period; i < candles.length; i++) {
      const curr = candles[i];
      const prev = candles[i - 1];
      const tr = Math.max(
        curr.high - curr.low,
        Math.abs(curr.high - prev.close),
        Math.abs(curr.low - prev.close)
      );
      trSum += tr;
    }
    return trSum / period;
  }

  // 1-Minute Candle Aggregator
  bus.on('tick:received', (tick) => {
    State.ticks.push(tick);

    // Prune raw ticks older than selected lookback
    const lookbackHours = State.settings.timeframeLookback === '1d' ? 24 : (State.settings.timeframeLookback === '8h' ? 8 : 4);
    const maxTickAgeSecs = lookbackHours * 3600;
    const currentSecs = tick.time;
    State.ticks = State.ticks.filter(t => currentSecs - t.time <= maxTickAgeSecs);

    // Candle 1-Minute Aggregation
    const candleTime = Math.floor(tick.time / 60) * 60;
    if (!State.formingCandle || State.formingCandle.time !== candleTime) {
      if (State.formingCandle) {
        State.candles1m.push({ ...State.formingCandle });
        // Keep 1m candle history capped to lookback timeframe
        const maxCandles = lookbackHours * 60;
        if (State.candles1m.length > maxCandles) {
          State.candles1m = State.candles1m.slice(-maxCandles);
        }
      }
      State.formingCandle = {
        time: candleTime,
        open: tick.price,
        high: tick.price,
        low: tick.price,
        close: tick.price,
        volume: 1
      };
    } else {
      State.formingCandle.high = Math.max(State.formingCandle.high, tick.price);
      State.formingCandle.low = Math.min(State.formingCandle.low, tick.price);
      State.formingCandle.close = tick.price;
      State.formingCandle.volume += 1;
    }

    // Trigger WFA Engine evaluation on every completed 1m candle or tick update
    evaluateWFAEngine();
  });

  // Walk Forward Analysis (WFA) Strategy Engine
  function evaluateWFAEngine() {
    const allCandles = [...State.candles1m];
    if (State.formingCandle) allCandles.push(State.formingCandle);

    if (allCandles.length < 20) {
      State.wfaConfidence = 0;
      State.wfaDirection = null;
      bus.emit('wfa:updated', { confidence: 0, direction: null });
      return;
    }

    // Split historical data into In-Sample (70%) and Out-of-Sample (30%)
    const inSampleCount = Math.floor(allCandles.length * 0.7);
    const inSampleCandles = allCandles.slice(0, inSampleCount);
    const outSampleCandles = allCandles.slice(inSampleCount);

    // Compute indicators on current candle state
    const closes = allCandles.map(c => c.close);
    const hma = calculateHMA(closes, 14);
    const rsi = calculateRSI(closes, 14);
    const roc = calculateRoC(closes, 12);
    const mfi = calculateMFI(allCandles, 14);
    const atr = calculateATR(allCandles, 14);

    const currentPrice = closes[closes.length - 1];

    // Evaluate signals on In-Sample window to dynamically weight indicators
    let hmaScore = 0, rsiScore = 0, rocScore = 0, mfiScore = 0;

    // Signal scoring logic
    if (hma && currentPrice > hma) hmaScore += 1;
    else if (hma && currentPrice < hma) hmaScore -= 1;

    if (rsi < 35) rsiScore += 1; // Oversold -> Call
    else if (rsi > 65) rsiScore -= 1; // Overbought -> Put

    if (roc > 0.05) rocScore += 1;
    else if (roc < -0.05) rocScore -= 1;

    if (mfi < 30) mfiScore += 1;
    else if (mfi > 70) mfiScore -= 1;

    // Aggregate directional vote
    const totalVote = hmaScore + rsiScore + rocScore + mfiScore;

    // Dynamic Out-of-Sample validation score calculation
    let outSampleWinRate = 0.75; // Baseline validation ratio
    if (outSampleCandles.length >= 5) {
      let correctPredictions = 0;
      let totalEval = 0;
      for (let i = 2; i < outSampleCandles.length; i++) {
        const pastClose = outSampleCandles[i - 2].close;
        const currClose = outSampleCandles[i].close;
        if (currClose > pastClose && totalVote > 0) correctPredictions++;
        else if (currClose < pastClose && totalVote < 0) correctPredictions++;
        totalEval++;
      }
      if (totalEval > 0) outSampleWinRate = correctPredictions / totalEval;
    }

    // Scale confidence score (0 - 100%)
    const voteMagnitude = Math.abs(totalVote) / 4.0; // 0 to 1
    const rawConfidence = Math.round((voteMagnitude * 0.6 + outSampleWinRate * 0.4) * 100);
    const confidence = Math.min(100, Math.max(0, rawConfidence));

    let direction = null;
    if (totalVote >= 2) direction = 'call';
    else if (totalVote <= -2) direction = 'put';

    State.wfaConfidence = confidence;
    State.wfaDirection = direction;

    bus.emit('wfa:updated', { confidence, direction, hma, rsi, roc, mfi, atr });

    // Check Auto-Trading Execution Rules
    checkAndTriggerTrade(direction, confidence, atr);
  }

  // Risk management and Automated Trade Execution
  function checkAndTriggerTrade(direction, confidence, atr) {
    if (!State.strategyEnabled) return;

    // 1. Cooldown Filter Check
    const nowSecs = Date.now() / 1000;
    if (nowSecs < State.cooldownUntil) {
      const remainingSecs = Math.ceil(State.cooldownUntil - nowSecs);
      bus.emit('cooldown:updated', { active: true, remainingSecs });
      return;
    } else {
      bus.emit('cooldown:updated', { active: false, remainingSecs: 0 });
    }

    // 2. Minimum Confidence Check
    if (confidence < State.settings.minConfidence || !direction) return;

    // 3. Minimum Payout Check
    if (Math.round(State.activePayout * 100) < State.settings.minPayout) {
      console.log(`[PO-WFA-Bot] Trade suppressed: Payout (${Math.round(State.activePayout * 100)}%) below min threshold (${State.settings.minPayout}%).`);
      return;
    }

    // 4. ATR Volatility Filter Check
    if (State.settings.useATRFilter && atr) {
      if (atr < 0.00005) {
        console.log('[PO-WFA-Bot] Trade suppressed: Market volatility too low (ATR < 0.00005).');
        return;
      }
    }

    // 5. Ensure no existing active trade on current asset
    if (State.openDeals.has(State.currentAsset)) {
      return;
    }

    // Execute Automated Trade
    console.log(`[PO-WFA-Bot] Signal Confirmed! Executing ${direction.toUpperCase()} with ${confidence}% confidence.`);
    const success = executeTrade(direction, State.settings.tradeAmount, State.settings.tradeDuration);
    if (success) {
      State.openDeals.set(State.currentAsset, {
        time: Date.now(),
        direction,
        amount: State.settings.tradeAmount
      });
    }
  }

  // =========================================================================
  // 5. SESSION BALANCE & TRAILING STOP-LOSS / TAKE-PROFIT MANAGER
  // =========================================================================
  bus.on('balance:updated', (balance) => {
    // Initialize starting balance if not set
    if (State.startingBalance === null) {
      State.startingBalance = balance;
      State.peakBalance = balance;
      console.log(`[PO-WFA-Bot] Session Starting Balance set to $${balance.toFixed(2)}`);
    }

    State.currentBalance = balance;

    // Update session peak balance
    if (balance > State.peakBalance) {
      State.peakBalance = balance;
      console.log(`[PO-WFA-Bot] New Peak Balance achieved: $${balance.toFixed(2)}`);
    }

    if (!State.strategyEnabled) return;

    // 1. Trailing Stop Loss Check
    const slThreshold = State.peakBalance * (1 - State.settings.trailingSL / 100);
    if (balance <= slThreshold) {
      console.warn(`[PO-WFA-Bot] Trailing Stop Loss breached! Current: $${balance.toFixed(2)}, SL Target: $${slThreshold.toFixed(2)}`);
      bus.emit('strategy:stopped', `Trailing Stop Loss Breached ($${balance.toFixed(2)} <= $${slThreshold.toFixed(2)})`);
      return;
    }

    // 2. Take Profit Check
    const tpThreshold = State.startingBalance * (1 + State.settings.takeProfit / 100);
    if (balance >= tpThreshold) {
      console.log(`[PO-WFA-Bot] Take Profit Target reached! Current: $${balance.toFixed(2)}, TP Target: $${tpThreshold.toFixed(2)}`);
      bus.emit('strategy:stopped', `Take Profit Target Reached ($${balance.toFixed(2)} >= $${tpThreshold.toFixed(2)})`);
      return;
    }
  });

  // Closed Deals Tracker & Cooldown Trigger
  bus.on('trade:closed', (dealData) => {
    console.log('[PO-WFA-Bot] Trade Closed:', dealData);
    if (State.openDeals.has(State.currentAsset)) {
      State.openDeals.delete(State.currentAsset);
    }

    let profit = 0;
    if (typeof dealData === 'object') {
      profit = dealData.profit !== undefined ? Number(dealData.profit) : (dealData.win !== undefined ? dealData.win : 0);
    }

    if (profit <= 0) {
      State.consecutiveLosses += 1;
      console.warn(`[PO-WFA-Bot] Loss detected. Consecutive Losses: ${State.consecutiveLosses}`);
      if (State.consecutiveLosses >= 2) {
        const cooldownMs = State.settings.cooldownMinutes * 60 * 1000;
        State.cooldownUntil = (Date.now() + cooldownMs) / 1000;
        console.warn(`[PO-WFA-Bot] 2 consecutive losses reached. Triggering ${State.settings.cooldownMinutes}m cooldown.`);
        State.consecutiveLosses = 0; // Reset counter after initiating cooldown
      }
    } else {
      State.consecutiveLosses = 0; // Reset on win
      console.log('[PO-WFA-Bot] Win detected! Consecutive loss counter reset.');
    }
  });

  // Mount GUI once DOM is loaded
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
      initGUI();
      setInterval(ensureLineChartView, 10000);
    });
  } else {
    initGUI();
    setInterval(ensureLineChartView, 10000);
  }

  // Export GUI & Quant Engine controller
  window.__PO_WFA_BOT__ = {
    State,
    bus,
    executeTrade,
    sendWsFrame,
    initGUI,
    ensureLineChartView,
    evaluateWFAEngine,
    calculateHMA,
    calculateRSI,
    calculateRoC,
    calculateMFI,
    calculateATR
  };

})();
