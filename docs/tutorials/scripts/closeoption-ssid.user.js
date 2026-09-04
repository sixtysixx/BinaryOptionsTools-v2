// ==UserScript==
// @name         CloseOption SSID Extractor
// @namespace    http://tampermonkey.net/
// @version      2.0
// @description  Extract CloseOption SSID (token|sid|demo|public_code|hidden_code) for BinaryOptionsToolsV2 - use Violentmonkey/Tampermonkey menu to extract and copy
// @author       Six
// @match        https://www.closeoption.com/*
// @match        https://closeoption.com/*
// @grant        GM_setClipboard
// @grant        GM_registerMenuCommand
// @grant        GM_notification
// @run-at       document-idle
// ==/UserScript==

(function() {
    'use strict';

    // State
    let ssid = null;
    let isDemo = false;

    // Detect if we're on a demo page
    function detectDemo() {
        const url = window.location.href.toLowerCase();
        return url.includes('demo') || url.includes('practice');
    }

    // Extract publicCode and hiddenCode from hidden inputs
    function extractCodes() {
        const publicCodeEl = document.querySelector('input[name="publicCode"], input[id="publicCode"]');
        const hiddenCodeEl = document.querySelector('input[name="hiddenCode"], input[id="hiddenCode"]');
        return {
            publicCode: publicCodeEl?.value || '',
            hiddenCode: hiddenCodeEl?.value || ''
        };
    }

    // Get JWT token from the API endpoint
    async function getJwtToken() {
        try {
            const response = await fetch('https://www.closeoption.com/api/v1/user/profile', {
                credentials: 'include',
                headers: { 'Accept': 'application/json' }
            });
            if (!response.ok) return null;
            const data = await response.json();
            return data.token || data.jwt || data.access_token || null;
        } catch (e) {
            console.error('[CloseOption SSID] Failed to get JWT token:', e);
            return null;
        }
    }

    // Establish socket.io connection to get sid
    function getSocketSid() {
        return new Promise((resolve) => {
            if (typeof io === 'undefined') {
                console.error('[CloseOption SSID] socket.io not loaded');
                resolve(null);
                return;
            }

            const socket = io('https://www.closeoption.com', {
                transports: ['websocket', 'polling'],
                withCredentials: true,
                autoConnect: true
            });

            let resolved = false;
            const timeout = setTimeout(() => {
                if (!resolved) {
                    resolved = true;
                    socket.disconnect();
                    resolve(null);
                }
            }, 10000);

            socket.on('connect', () => {
                if (!resolved) {
                    resolved = true;
                    clearTimeout(timeout);
                    const sid = socket.id;
                    socket.disconnect();
                    resolve(sid);
                }
            });

            socket.on('connect_error', (err) => {
                if (!resolved) {
                    resolved = true;
                    clearTimeout(timeout);
                    console.error('[CloseOption SSID] Socket connection error:', err);
                    resolve(null);
                }
            });
        });
    }

    // Build SSID string
    function buildSsid(token, sid, demo, publicCode, hiddenCode) {
        return `${token}|${sid}|${demo ? '1' : '0'}|${publicCode}|${hiddenCode}`;
    }

    // Main extraction function
    async function extractSsid() {
        console.log('[CloseOption SSID] Starting extraction...');

        isDemo = detectDemo();
        console.log(`[CloseOption SSID] Account type: ${isDemo ? 'DEMO' : 'REAL'}`);

        // Get codes from page
        const { publicCode, hiddenCode } = extractCodes();
        if (!publicCode || !hiddenCode) {
            const msg = 'Missing publicCode or hiddenCode. Navigate to trade room first.';
            console.error('[CloseOption SSID]', msg);
            notify('CloseOption SSID', msg, 'error');
            return null;
        }
        console.log(`[CloseOption SSID] Found codes: public=${publicCode.slice(0, 10)}..., hidden=${hiddenCode.slice(0, 10)}...`);

        // Get JWT token
        const token = await getJwtToken();
        if (!token) {
            const msg = 'Failed to get JWT token. Make sure you are logged in.';
            console.error('[CloseOption SSID]', msg);
            notify('CloseOption SSID', msg, 'error');
            return null;
        }
        console.log(`[CloseOption SSID] Got JWT token: ${token.slice(0, 20)}...`);

        // Get socket sid
        const sid = await getSocketSid();
        if (!sid) {
            const msg = 'Failed to get socket sid. Make sure socket.io connection is established.';
            console.error('[CloseOption SSID]', msg);
            notify('CloseOption SSID', msg, 'error');
            return null;
        }
        console.log(`[CloseOption SSID] Got socket sid: ${sid}`);

        // Build SSID
        ssid = buildSsid(token, sid, isDemo, publicCode, hiddenCode);
        console.log(`[CloseOption SSID] Extracted SSID: ${ssid.slice(0, 50)}...`);

        return ssid;
    }

    // Copy to clipboard
    function copyToClipboard(text) {
        if (typeof GM_setClipboard !== 'undefined') {
            GM_setClipboard(text);
            return Promise.resolve();
        }
        return navigator.clipboard.writeText(text).catch(() => {
            const ta = document.createElement('textarea');
            ta.value = text;
            ta.style.position = 'fixed';
            ta.style.opacity = '0';
            document.body.appendChild(ta);
            ta.select();
            document.execCommand('copy');
            document.body.removeChild(ta);
        });
    }

    // Notify user (GM_notification or console)
    function notify(title, text, type = 'info') {
        if (typeof GM_notification !== 'undefined') {
            GM_notification({ title, text, timeout: 3000, type });
        } else {
            console.log(`[${title}] ${text}`);
        }
    }

    // Register menu command (Violentmonkey/Tampermonkey)
    if (typeof GM_registerMenuCommand !== 'undefined') {
        GM_registerMenuCommand('Extract & Copy CloseOption SSID', async () => {
            const result = await extractSsid();
            if (result) {
                try {
                    await copyToClipboard(result);
                    notify('CloseOption SSID', 'SSID copied to clipboard!', 'success');
                } catch (e) {
                    notify('CloseOption SSID', 'Failed to copy to clipboard', 'error');
                }
            }
        });
    }

    console.log('[CloseOption SSID] Ready. Use Violentmonkey/Tampermonkey menu → "Extract & Copy CloseOption SSID"');
})();