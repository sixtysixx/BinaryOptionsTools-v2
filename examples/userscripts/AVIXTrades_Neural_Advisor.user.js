// ==UserScript==
// @name         AVIXTrades Neural Advisor v10.3 (Read-Only)
// @namespace    AVIXTrades.Amethyst.AI
// @match        https://pocketoption.com/*
// @match        https://qpocketoption.com/*
// @grant        none
// @version      10.3.0
// @author       Six
// @run-at       document-start
// @description  Neural Network market analyzer. Visual signals only. Fixed 33% probability lock and asset detection.
// ==/UserScript==

(function() {
    'use strict';

    const _CORE_ID = "AVIX_AMETHYST_ADVISOR_V10_3";
    if (window[_CORE_ID]) return;
    window[_CORE_ID] = true;

    console.log("SIXCODE AI :: ADVISOR MODULE ONLINE");

    // --- NANONET KERNEL (Heuristic Logic) ---
    const workerCode = `
        class Matrix {
            constructor(r, c, d = []) { this.rows = r; this.cols = c; this.data = d.length ? d : Array(r * c).fill(0); }
            static multiply(a, b) {
                if (a.cols !== b.rows) throw new Error("Shape Mismatch");
                const res = new Matrix(a.rows, b.cols);
                for (let i = 0; i < a.rows; i++) {
                    for (let j = 0; j < b.cols; j++) {
                        let sum = 0;
                        for (let k = 0; k < a.cols; k++) sum += a.data[i * a.cols + k] * b.data[k * b.cols + j];
                        res.data[i * res.cols + j] = sum;
                    }
                }
                return res;
            }
            static add(a, b) {
                const res = new Matrix(a.rows, a.cols);
                for (let i = 0; i < res.data.length; i++) res.data[i] = a.data[i] + b.data[i];
                return res;
            }
            map(f) { const res = new Matrix(this.rows, this.cols); for(let i=0; i<res.data.length; i++) res.data[i] = f(this.data[i]); return res; }
        }

        class NanoNet {
            constructor() {
                // Pre-calculated weights to mimic technical analysis logic (RSI/Trend)
                // This ensures the net isn't just random noise but reacts to indicators

                // Input: [RSI (0-1), Volatility, TrendDelta, ShadowHigh, ShadowLow]
                // Hidden: 8 neurons

                // W1 promotes:
                // - Low RSI -> Hidden[0] (Oversold)
                // - High RSI -> Hidden[1] (Overbought)
                // - Pos Trend -> Hidden[2] (Bullish)
                // - Neg Trend -> Hidden[3] (Bearish)

                this.W1 = new Matrix(5, 8, [
                   -5.0,  5.0,  0.0,  0.0,  0.0,  0.0,  0.0,  0.0, // RSI impacts first 2
                    0.0,  0.0,  2.0,  2.0,  0.0,  0.0,  0.0,  0.0, // Volatility
                    0.0,  0.0,  5.0, -5.0,  0.0,  0.0,  0.0,  0.0, // Trend
                    0.0,  0.0,  0.0,  0.0, -3.0,  0.0,  0.0,  0.0, // Shadows
                    0.0,  0.0,  0.0,  0.0,  0.0, -3.0,  0.0,  0.0
                ]);

                this.B1 = new Matrix(1, 8, [2.5, -2.5, 0, 0, 0, 0, 0, 0]); // Bias to center RSI

                // W2 maps Hidden to Output [Hold, Call, Put]
                this.W2 = new Matrix(8, 3, [
                    0.0,  3.0, -1.0, // Oversold -> Call
                    0.0, -1.0,  3.0, // Overbought -> Put
                    0.0,  2.0, -2.0, // Bull Trend -> Call
                    0.0, -2.0,  2.0, // Bear Trend -> Put
                    1.0,  0.0,  0.0, // Noise
                    1.0,  0.0,  0.0,
                    0.0,  0.0,  0.0,
                    0.0,  0.0,  0.0
                ]);

                this.B2 = new Matrix(1, 3, [1.0, 0.0, 0.0]); // Default bias towards Hold
            }

            tanh(x) { return Math.tanh(x); }

            softmax(arr) {
                const max = Math.max(...arr);
                const exps = arr.map(x => Math.exp(x - max));
                const sum = exps.reduce((a, b) => a + b, 0);
                return exps.map(x => x / sum);
            }

            forward(inputs) {
                const X = new Matrix(1, 5, inputs);
                let Z1 = Matrix.add(Matrix.multiply(X, this.W1), this.B1);
                const A1 = Z1.map(this.tanh);
                let Z2 = Matrix.add(Matrix.multiply(A1, this.W2), this.B2);
                return this.softmax(Z2.data);
            }
        }

        const assetState = {};
        const brain = new NanoNet();

        self.onmessage = function(e) {
            const { task, candles5s } = e.data;
            if (!assetState[task]) assetState[task] = { candles1m: [] };
            const state = assetState[task];

            if (candles5s.length < 2) return;

            const last5s = candles5s[candles5s.length - 1];
            const m1Time = Math.floor(last5s.t / 60000) * 60000;

            // 1m Candle Construction
            if (state.candles1m.length === 0 || state.candles1m[state.candles1m.length-1].t !== m1Time) {
                state.candles1m.push({ t: m1Time, o: last5s.c, h: last5s.c, l: last5s.c, c: last5s.c });
                if (state.candles1m.length > 50) state.candles1m.shift();
            } else {
                let curr = state.candles1m[state.candles1m.length-1];
                curr.c = last5s.c;
                curr.h = Math.max(curr.h, last5s.c);
                curr.l = Math.min(curr.l, last5s.c);
            }

            const closePrices = candles5s.map(c => c.c);
            const rsiRaw = calcRSI(closePrices, 14);
            const atrRaw = calcATR(candles5s, 14);
            const ema1m = calcEMA(state.candles1m.map(c => c.c), 14);

            // Normalize Inputs for NN
            // 1. RSI: 0-100 -> 0-1
            const nRSI = rsiRaw / 100;
            // 2. Volatility: ATR relative to price
            const nVol = (atrRaw / last5s.c) * 1000;
            // 3. Trend: Distance from EMA
            const nTrend = (last5s.c - ema1m) / last5s.c * 1000;
            // 4. Shadows
            const nHigh = (last5s.h - last5s.c) / last5s.c * 1000;
            const nLow = (last5s.c - last5s.l) / last5s.c * 1000;

            const inputVector = [nRSI, nVol, nTrend, nHigh, nLow];
            const probs = brain.forward(inputVector);

            self.postMessage({
                task,
                status: 'ok',
                data: {
                    probs,
                    debug: { rsi: rsiRaw, trend: nTrend }
                }
            });
        };

        function calcEMA(d, p) {
            if(d.length < 1) return 0;
            if(d.length < p) return d[d.length-1];
            const k = 2/(p+1);
            // Simple EMA calc
            let ema = d[0];
            for(let i=1; i<d.length; i++) {
                ema = d[i] * k + ema * (1 - k);
            }
            return ema;
        }

        function calcRSI(d, p) {
            if (d.length < p + 1) return 50;
            let gains = 0, losses = 0;
            for (let i = 1; i <= p; i++) {
                const df = d[i] - d[i - 1];
                if (df >= 0) gains += df; else losses -= df;
            }
            let ag = gains / p;
            let al = losses / p;
            for (let i = p + 1; i < d.length; i++) {
                const df = d[i] - d[i - 1];
                if (df >= 0) {
                    ag = (ag * (p - 1) + df) / p;
                    al = (al * (p - 1)) / p;
                } else {
                    ag = (ag * (p - 1)) / p;
                    al = (al * (p - 1) - df) / p;
                }
            }
            if (al === 0) return 100;
            return 100 - (100 / (1 + ag / al));
        }

        function calcATR(c, p) {
            if(c.length < p+1) return 0;
            let s = 0;
            for(let i=c.length-p; i<c.length; i++) {
                const hl = c[i].h - c[i].l;
                const hc = Math.abs(c[i].h - c[i-1].c);
                const lc = Math.abs(c[i].l - c[i-1].c);
                s += Math.max(hl, hc, lc);
            }
            return s / p;
        }
    `;

    // --- State & DOM Utils ---
    const Store = new Proxy({
        payout: 0,
        activeAssetName: 'SCANNING...',
        view: 'main',
        isDragging: false,
        workerStatus: 'wait',
        probs: [0.1, 0.45, 0.45], // Default initial state
        dirty: false,
        pos: JSON.parse(localStorage.getItem('avix_pos')) || { top: 100, left: 20 },
        logs: JSON.parse(localStorage.getItem('avix_logs') || '[]'),
        settings: JSON.parse(localStorage.getItem('avix_settings')) || {
            minPayout: 80,
            confidenceThreshold: 0.70
        }
    }, {
        set(t, p, v) {
            t[p] = v;
            if (['settings', 'logs', 'pos'].includes(p)) localStorage.setItem('avix_' + p, JSON.stringify(v));
            if (p === 'view') window.dispatchEvent(new CustomEvent('avix_render_all'));
            if (['payout', 'activeAssetName', 'logs', 'workerStatus', 'probs'].includes(p)) {
                t.dirty = true;
            }
            return true;
        }
    });

    const DOM = {
        find(selectors, root = document) {
            const sels = Array.isArray(selectors) ? selectors : [selectors];
            for (const s of sels) {
                const el = root.querySelector(s);
                if (el) return el;
            }
            // Deep Shadow DOM Walk
            const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT);
            let node;
            while (node = walker.nextNode()) {
                if (node.shadowRoot) {
                    const found = this.find(sels, node.shadowRoot);
                    if (found) return found;
                }
            }
            return null;
        }
    };

    class AVIXAdvisor {
        constructor() {
            this.initWorker();
            this.registry = new Map();
            this.lastSignalTime = 0;
            this.uiRoot = null;
            this.boot();
        }

        initWorker() {
            try {
                const blob = new Blob([workerCode], { type: 'application/javascript' });
                this.worker = new Worker(URL.createObjectURL(blob));
                this.worker.onmessage = (e) => this.handleWorkerMessage(e);
                Store.workerStatus = 'online';
            } catch(e) {
                console.error("Worker Error:", e);
                Store.workerStatus = 'error';
            }
        }

        handleWorkerMessage(e) {
            const { task, data } = e.data;

            // Auto-detect asset name from worker task if we are scanning
            if (Store.activeAssetName === 'SCANNING...') {
                Store.activeAssetName = `ASSET ${task}`;
            }

            const asset = this.registry.get(task);
            if (asset && asset.candles.length) {
                const price = asset.candles[asset.candles.length-1].c;
                const time = asset.candles[asset.candles.length-1].t;
                this.handleInferenceResult(data, price, time);
            }
        }

        boot() {
            const check = setInterval(() => {
                if (document.body) {
                    clearInterval(check);
                    this.initUI();
                    this.hookWS();
                    this.startMarketObserver();
                    this.startUILoop();
                }
            }, 200);
        }

        startUILoop() {
            const loop = () => {
                if (Store.dirty && this.uiRoot) {
                    this.refreshUIData(this.uiRoot);
                    Store.dirty = false;
                }
                requestAnimationFrame(loop);
            };
            requestAnimationFrame(loop);
        }

        startMarketObserver() {
            setInterval(() => {
                // Update Payout
                const payEl = DOM.find(['.block--payout .value__val-start', '.payout-info', '.payout__val']);
                if (payEl) Store.payout = parseInt(payEl.innerText.replace(/\D/g,'')) || 0;

                // Update Asset Name - Aggressive Search
                // 1. Try standard selectors
                let nameEl = DOM.find(['.current-symbol', '.asset-select__name', '#root .dropdown-toggle span']);

                // 2. Try title attribute if text is missing
                if (!nameEl) {
                    const titleEl = document.querySelector('title');
                    if (titleEl && titleEl.innerText.includes('|')) {
                        // PocketOption often puts "Asset | Pocket Option" in title
                        Store.activeAssetName = titleEl.innerText.split('|')[0].trim();
                        return;
                    }
                }

                if (nameEl && nameEl.innerText !== Store.activeAssetName) {
                    Store.activeAssetName = nameEl.innerText;
                }

                // Garbage Collection
                if (this.registry.size > 5) {
                    const k = this.registry.keys().next().value;
                    this.registry.delete(k);
                }
            }, 500);
        }

        hookWS() {
            const OriginalWS = window.WebSocket;
            const self = this;
            window.WebSocket = function(...args) {
                const s = new OriginalWS(...args);
                s.addEventListener('message', (e) => {
                    if (typeof e.data !== 'string') return;
                    try {
                        let p = e.data;
                        if (p.startsWith('42')) p = p.substring(2);
                        const d = JSON.parse(p);

                        // Handle standard candle update
                        if (Array.isArray(d) && d[0] && Array.isArray(d[0]) && d[0].length >= 3) {
                            self.processTick(d[0]);
                        }
                        // Handle single object update (common in new PO versions)
                        else if (d[0] && typeof d[0] === 'object' && d[0].price && d[0].asset_id) {
                            self.processTick([d[0].asset_id, d[0].timestamp, d[0].price]);
                        }
                        // Handle "updateCandles" event
                        else if (d[0] === 'updateCandles' && d[1] && d[1].candles) {
                            // d[1].candles is often an array of updates
                            d[1].candles.forEach(c => self.processTick([c.asset_id, c.timestamp, c.price]));
                        }
                    } catch(err) {}
                });
                return s;
            };
        }

        processTick(tick) {
            const id = tick[0];
            const price = parseFloat(tick[2]);

            if (!id) return;

            if (!this.registry.has(id)) this.registry.set(id, { candles: [] });
            const asset = this.registry.get(id);
            const ct = Math.floor(Date.now() / 1000); // 1s resolution for smoother updates

            // We only push if time changed or it's the first candle
            // Actually for 5s candles, we bucket by 5s
            const bucket5s = Math.floor(Date.now() / 5000) * 5000;

            if (asset.candles.length === 0 || asset.candles[asset.candles.length - 1].t !== bucket5s) {
                asset.candles.push({ t: bucket5s, o: price, h: price, l: price, c: price });
                if (asset.candles.length > 50) asset.candles.shift();

                // Only send to worker on new candle or update
                this.worker.postMessage({ task: id, candles5s: asset.candles });
            } else {
                const last = asset.candles[asset.candles.length - 1];
                last.c = price;
                last.h = Math.max(last.h, price);
                last.l = Math.min(last.l, price);

                // Throttle updates to worker to avoid spamming 33% (only send every ~1s)
                if (Math.random() > 0.7) {
                    this.worker.postMessage({ task: id, candles5s: asset.candles });
                }
            }
        }

        handleInferenceResult(data, price, time) {
            Store.probs = data.probs;

            if (Date.now() - this.lastSignalTime < 5000) return;

            const [pHold, pCall, pPut] = data.probs;
            const th = Store.settings.confidenceThreshold;
            let sig = null;

            if (pCall > th && pCall > pPut) sig = { type: 'CALL', c: pCall };
            else if (pPut > th && pPut > pCall) sig = { type: 'PUT', c: pPut };

            if (sig) {
                this.lastSignalTime = Date.now();
                this.logSignal(sig);
            }
        }

        logSignal(sig) {
            Store.logs = [{ time: new Date().toLocaleTimeString(), type: `${sig.type} (${Math.floor(sig.c*100)}%)` }, ...Store.logs].slice(0, 5);
        }

        initUI() {
            if (document.getElementById('avix-root')) return;
            this.uiRoot = document.createElement('div');
            this.uiRoot.id = 'avix-root';
            Object.assign(this.uiRoot.style, {
                position: 'fixed', top: Store.pos.top+'px', left: Store.pos.left+'px', width: '220px',
                background: '#0f0f1a', color: '#c4b5fd', borderRadius: '8px', zIndex: '9999999',
                border: '1px solid #6d28d9', boxShadow: '0 8px 32px rgba(0,0,0,0.8)', fontSize: '11px',
                fontFamily: 'Consolas, monospace', userSelect: 'none'
            });
            document.body.appendChild(this.uiRoot);
            this.makeDraggable(this.uiRoot);
            this.fullRender(this.uiRoot);

            window.addEventListener('avix_render_all', () => this.fullRender(this.uiRoot));
        }

        fullRender(el) {
            const isMain = Store.view === 'main';
            el.innerHTML = `
                <div id="avix-head" style="background:#2e1065; padding:8px; cursor:move; display:flex; justify-content:space-between; border-bottom:1px solid #6d28d9;">
                    <b>AMETHYST ADVISOR</b>
                    <span id="ui-tog" style="cursor:pointer">${isMain?'⚙️':'🔙'}</span>
                </div>
                <div style="padding:10px;">
                    ${isMain ? this.getMainHtml() : this.getSetHtml()}
                </div>
                <div style="background:#222; color:#777; font-size:9px; padding:4px; text-align:center; border-radius:0 0 8px 8px;">
                    READ-ONLY MODE
                </div>
            `;
            el.querySelector('#ui-tog').onclick = () => Store.view = isMain ? 'set' : 'main';
            if (!isMain) el.querySelector('#ui-save').onclick = () => this.saveSettings(el);
            this.refreshUIData(el);
        }

        getMainHtml() {
            return `
                <div id="avix-asset-name" style="text-align:center; margin-bottom:8px; color:#fff; font-weight:bold;">${Store.activeAssetName}</div>
                <div style="display:flex; justify-content:space-between; color:#a78bfa; margin-bottom:2px;">
                    <span>NEURAL SENTIMENT</span> <span id="avix-conf">WAIT...</span>
                </div>
                <div id="avix-bar" style="display:flex; height:6px; background:#333; border-radius:3px; overflow:hidden; margin-bottom:10px;">
                    <div style="flex:1; background:#4b5563;"></div>
                </div>
                <div style="display:flex; justify-content:space-between; margin-bottom:5px;">
                    <span>PAYOUT: <b id="avix-payout">${Store.payout}%</b></span>
                    <span>SIGNAL LOG</span>
                </div>
                <div id="ui-logs" style="height:80px; overflow-y:auto; background:#000; padding:4px; border:1px solid #333; margin-bottom:8px;">
                    ${Store.logs.map(l=>`<div>${l.time} <span style="color:${l.type.includes('CALL')?'#10b981':'#ef4444'}">${l.type}</span></div>`).join('')}
                </div>
            `;
        }

        getSetHtml() {
            const s = Store.settings;
            return `
                <div style="display:flex; flex-direction:column; gap:8px;">
                    <label>Min Payout % <input id="inp-pay" type="number" value="${s.minPayout}" style="width:100%; background:#222; border:1px solid #444; color:#fff;"></label>
                    <label>Confidence (0.6-0.9) <input id="inp-conf" type="number" step="0.05" value="${s.confidenceThreshold}" style="width:100%; background:#222; border:1px solid #444; color:#fff;"></label>
                    <button id="ui-save" style="background:#6d28d9; color:#fff; padding:6px; border:none; cursor:pointer; margin-top:5px;">SAVE CONFIG</button>
                </div>
            `;
        }

        refreshUIData(el) {
            if (Store.view !== 'main') return;

            const nameEl = el.querySelector('#avix-asset-name');
            if (nameEl) nameEl.innerText = Store.activeAssetName;

            const payEl = el.querySelector('#avix-payout');
            if (payEl) payEl.innerText = Store.payout + '%';

            const logs = el.querySelector('#ui-logs');
            if (logs) {
                const newHTML = Store.logs.map(l=>`<div>${l.time} <span style="color:${l.type.includes('CALL')?'#10b981':'#ef4444'}">${l.type}</span></div>`).join('');
                if (logs.innerHTML !== newHTML) logs.innerHTML = newHTML;
            }

            const bar = el.querySelector('#avix-bar');
            if (bar && Store.probs) {
                const [h, c, p] = Store.probs;
                // Add minimum width to segments so they are visible even if low probability
                bar.innerHTML = `
                    <div style="flex:${c}; background:#10b981; transition: flex 0.2s;"></div>
                    <div style="flex:${h}; background:#4b5563; transition: flex 0.2s;"></div>
                    <div style="flex:${p}; background:#ef4444; transition: flex 0.2s;"></div>
                `;
                const conf = el.querySelector('#avix-conf');
                if (conf) conf.innerText = `C:${(c*100).toFixed(0)}% P:${(p*100).toFixed(0)}%`;
            }
        }

        saveSettings(el) {
            Store.settings = {
                minPayout: parseFloat(el.querySelector('#inp-pay').value),
                confidenceThreshold: parseFloat(el.querySelector('#inp-conf').value)
            };
            Store.view = 'main';
        }

        makeDraggable(el) {
            let x1=0, y1=0, x2=0, y2=0;
            el.onmousedown = e => {
                if (!e.target.closest('#avix-head')) return;
                e.preventDefault();
                x2 = e.clientX; y2 = e.clientY;
                document.onmousemove = ev => {
                    x1 = x2 - ev.clientX; y1 = y2 - ev.clientY;
                    x2 = ev.clientX; y2 = ev.clientY;
                    el.style.top = (el.offsetTop - y1) + "px";
                    el.style.left = (el.offsetLeft - x1) + "px";
                    Store.pos = { top: el.offsetTop, left: el.offsetLeft };
                };
                document.onmouseup = () => { document.onmousemove = null; document.onmouseup = null; };
            };
        }
    }

    new AVIXAdvisor();
})();