// ==UserScript==
// @name         BinaryOptionsTools SSID Extractor
// @namespace    SixsBinaryOptionsSSIDFetcher
// @version      3.1
// @description  Extract SSID/credentials for PocketOption and CloseOption for BinaryOptionsToolsV2 - use Violentmonkey/Tampermonkey menu to extract and copy
// @author       Six
// @match        *://pocketoption.com/*
// @match        *://*.pocketoption.com/*
// @match        *://closeoption.com/*
// @match        *://*.closeoption.com/*
// @grant        GM_setClipboard
// @grant        GM_registerMenuCommand
// @grant        GM_notification
// @grant        GM_getValue
// @grant        GM_setValue
// @run-at       document-start
// ==/UserScript==

(function() {
    'use strict';

    // Platform detection
    function detectPlatform() {
        const hostname = window.location.hostname.toLowerCase();
        if (hostname.includes('pocketoption.com')) return 'pocketoption';
        if (hostname.includes('closeoption.com')) return 'closeoption';
        return 'unknown';
    }

    // Detect if we're on a demo page
    function detectDemo() {
        const url = window.location.href.toLowerCase();
        return url.includes('demo') || url.includes('practice');
    }

    // Platform-specific state
    const platform = detectPlatform();
    let pocketOptionSsid = null;
    let pocketOptionAuthMessage = null;
    let closeOptionCreds = null;

    // ==================== POCKETOPTION ====================

    // Extract SSID from intercepted auth message
    function extractPocketOptionSsid(authData) {
        try {
            // Auth message format: 42["auth",{"session":"...","isDemo":1,"uid":123,"platform":2,"isFastHistory":true,"isOptimized":true}]
            // Handle potential whitespace and multiple concatenated messages
            const match = authData.match(/42\["auth",\s*(\{.+?\})\s*\]/);
            if (!match) return null;

            const payload = JSON.parse(match[1]);
            const token = payload.token || payload.sessionToken || payload.session || payload.ssid || '';
            const demo = payload.isDemo !== undefined ? payload.isDemo : detectDemo();

            if (!token) return null;

            // PocketOption SSID format: token|isDemo
            return `${token}|${demo ? '1' : '0'}`;
        } catch (e) {
            console.error('[PO SSID] Failed to parse auth message:', e, 'Data:', authData.substring(0, 200));
            return null;
        }
    }

    // ==================== CLOSEOPTION ====================

    // Extract CloseOption credentials from localStorage/cookies
    function extractCloseOptionCreds() {
        try {
            // Try to get from localStorage first
            const token = localStorage.getItem('token') || getCookie('token');
            const sid = localStorage.getItem('sid') || getCookie('sid');
            const publicCode = localStorage.getItem('public_code') || localStorage.getItem('publicCode') || getCookie('public_code') || getCookie('publicCode');
            const hiddenCode = localStorage.getItem('hidden_code') || localStorage.getItem('hiddenCode') || getCookie('hidden_code') || getCookie('hiddenCode');

            // Also check for alternative storage keys
            const altToken = localStorage.getItem('auth_token') || localStorage.getItem('access_token');
            const altSid = localStorage.getItem('socket_sid') || localStorage.getItem('session_id');

            const finalToken = token || altToken;
            const finalSid = sid || altSid;

            if (!finalToken || !finalSid || !publicCode || !hiddenCode) {
                console.log('[CO SSID] Missing credentials:', { token: !!finalToken, sid: !!finalSid, publicCode: !!publicCode, hiddenCode: !!hiddenCode });
                return null;
            }

            const demo = detectDemo() ? '1' : '0';

            // CloseOption credential format: token|sid|publicCode|hiddenCode|isDemo
            return `${finalToken}|${finalSid}|${publicCode}|${hiddenCode}|${demo}`;
        } catch (e) {
            console.error('[CO SSID] Failed to extract credentials:', e);
            return null;
        }
    }

    // Helper to get cookie by name
    function getCookie(name) {
        const value = `; ${document.cookie}`;
        const parts = value.split(`; ${name}=`);
        if (parts.length === 2) return parts.pop().split(';').shift();
        return null;
    }

    // ==================== WEBSOCKET HOOKING ====================

    // Hook a single WebSocket instance
    function hookSocket(socket, url) {
        if (socket._hooked) return;
        socket._hooked = true;

        try {
            socket._interceptUrl = url.toString();
        } catch (e) {}

        const originalSend = socket.send;
        socket.send = function(data) {
            const result = originalSend.apply(this, arguments);

            const rawSocketUrl = this.url || this._interceptUrl || '';
            
            // Skip events-po.com (PocketOption analytics)
            let socketHost = '';
            try {
                socketHost = new URL(rawSocketUrl, window.location.href).hostname.toLowerCase();
            } catch (e) {}
            if (socketHost === 'events-po.com' || socketHost.endsWith('.events-po.com')) {
                return result;
            }

            // Log ALL PocketOption WebSocket traffic for debugging
            if (platform === 'pocketoption' && typeof data === 'string') {
                console.log('[PO SSID] OUTGOING:', data.substring(0, 500));
                
                // Check for auth in outgoing messages
                if (data.includes('"auth"')) {
                    const match = data.match(/42\["auth",\s*(\{.+?\})\s*\]/);
                    if (match) {
                        pocketOptionAuthMessage = match[0];
                        pocketOptionSsid = extractPocketOptionSsid(match[0]);
                        if (pocketOptionSsid) {
                            console.log('[PO SSID] Auth intercepted (send), SSID ready:', pocketOptionSsid.substring(0, 20) + '...');
                        }
                    }
                }
            }

            // Log CloseOption events for debugging
            if (platform === 'closeoption' && typeof data === 'string' && data.startsWith('42[')) {
                console.log('[CO SSID] Outgoing event:', data.substring(0, 100));
            }

            return result;
        };

        // Also hook onmessage to catch incoming PocketOption auth
        const originalOnMessage = socket.onmessage;
        socket.onmessage = function(event) {
            if (platform === 'pocketoption' && typeof event.data === 'string') {
                console.log('[PO SSID] INCOMING:', event.data.substring(0, 500));
                
                // Check for auth in incoming messages (may be concatenated)
                if (event.data.includes('"auth"')) {
                    const match = event.data.match(/42\["auth",\s*(\{.+?\})\s*\]/);
                    if (match) {
                        pocketOptionAuthMessage = match[0];
                        pocketOptionSsid = extractPocketOptionSsid(match[0]);
                        if (pocketOptionSsid) {
                            console.log('[PO SSID] Auth received (onmessage), SSID ready:', pocketOptionSsid.substring(0, 20) + '...');
                        }
                    }
                }
            }
            if (originalOnMessage) originalOnMessage.call(this, event);
        };
    }

    // Hook WebSocket constructor
    function hookWebSocket() {
        const OriginalWebSocket = window.WebSocket;

        window.WebSocket = function(url, protocols) {
            const socket = new OriginalWebSocket(url, protocols);
            hookSocket(socket, url);
            return socket;
        };

        // Copy static properties
        Object.getOwnPropertyNames(OriginalWebSocket).forEach(prop => {
            if (prop !== 'prototype') {
                Object.defineProperty(window.WebSocket, prop, Object.getOwnPropertyDescriptor(OriginalWebSocket, prop));
            }
        });
        Object.getOwnPropertySymbols(OriginalWebSocket).forEach(sym => {
            Object.defineProperty(window.WebSocket, sym, Object.getOwnPropertyDescriptor(OriginalWebSocket, sym));
        });

        window.WebSocket.prototype = OriginalWebSocket.prototype;
        window.WebSocket.prototype.constructor = window.WebSocket;

        console.log(`[SSID] WebSocket constructor hooked for ${platform}`);
    }

    // Scan for existing WebSocket instances on the page
    function scanExistingWebSockets() {
        // Check for WebSocket instances stored in common places
        const checkObj = (obj, path) => {
            if (!obj || typeof obj !== 'object') return;
            if (obj instanceof WebSocket) {
                console.log('[PO SSID] Found existing WebSocket at:', path);
                hookSocket(obj, obj.url || 'unknown');
            }
            // Recursively check properties (but avoid circular refs)
            const seen = new WeakSet();
            const recurse = (o, p) => {
                if (!o || typeof o !== 'object' || seen.has(o)) return;
                seen.add(o);
                for (const key of Object.keys(o)) {
                    const val = o[key];
                    if (val instanceof WebSocket) {
                        console.log('[PO SSID] Found existing WebSocket at:', p + '.' + key);
                        hookSocket(val, val.url || 'unknown');
                    } else if (val && typeof val === 'object') {
                        recurse(val, p + '.' + key);
                    }
                }
            };
            recurse(obj, path);
        };

        // Check common global objects that might hold WebSocket references
        checkObj(window, 'window');
        checkObj(window.__PO_WS__, 'window.__PO_WS__');
        checkObj(window.socket, 'window.socket');
        checkObj(window.ws, 'window.ws');
        checkObj(window.websocket, 'window.websocket');
    }

    // ==================== CLIPBOARD & NOTIFICATIONS ====================

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

    // ==================== MENU COMMANDS ====================

    // Register menu commands (Violentmonkey/Tampermonkey)
    if (typeof GM_registerMenuCommand !== 'undefined') {
        // PocketOption: Extract & Copy SSID
        if (platform === 'pocketoption') {
            GM_registerMenuCommand('Extract & Copy PocketOption SSID', async () => {
                if (!pocketOptionSsid) {
                    notify('PocketOption SSID', 'No auth message intercepted yet. Make sure you are logged in and the WebSocket has connected.', 'warning');
                    return;
                }

                try {
                    await copyToClipboard(pocketOptionSsid);
                    notify('PocketOption SSID', 'SSID copied to clipboard!', 'success');
                } catch (e) {
                    notify('PocketOption SSID', 'Failed to copy to clipboard', 'error');
                }
            });

            // Debug command for PocketOption
            GM_registerMenuCommand('Debug: Show PocketOption WebSocket Traffic', () => {
                console.log('[PO SSID] Last auth message:', pocketOptionAuthMessage);
                console.log('[PO SSID] Extracted SSID:', pocketOptionSsid);
                notify('PocketOption Debug', 'Check console for WebSocket traffic', 'info');
            });

            // Force re-scan for WebSockets
            GM_registerMenuCommand('Debug: Re-scan for WebSockets', () => {
                scanExistingWebSockets();
                notify('PocketOption Debug', 'Re-scanned for WebSocket instances', 'info');
            });
        }

        // CloseOption: Extract & Copy Credentials
        if (platform === 'closeoption') {
            GM_registerMenuCommand('Extract & Copy CloseOption Credentials', async () => {
                const creds = extractCloseOptionCreds();
                if (!creds) {
                    notify('CloseOption Credentials', 'Could not find all required credentials (token, sid, public_code, hidden_code). Make sure you are logged in.', 'warning');
                    return;
                }

                try {
                    await copyToClipboard(creds);
                    notify('CloseOption Credentials', 'Credentials copied to clipboard! Format: token|sid|publicCode|hiddenCode|isDemo', 'success');
                } catch (e) {
                    notify('CloseOption Credentials', 'Failed to copy to clipboard', 'error');
                }
            });

            // Also add a debug command to show what's in localStorage
            GM_registerMenuCommand('Debug: Show CloseOption Storage', () => {
                const keys = ['token', 'sid', 'public_code', 'publicCode', 'hidden_code', 'hiddenCode', 'auth_token', 'access_token', 'socket_sid', 'session_id'];
                const found = {};
                keys.forEach(k => {
                    const v = localStorage.getItem(k);
                    if (v) found[k] = v.substring(0, 30) + (v.length > 30 ? '...' : '');
                });
                console.log('[CO SSID] localStorage:', found);
                console.log('[CO SSID] Cookies:', document.cookie);
                notify('CloseOption Debug', 'Check console for storage contents', 'info');
            });
        }
    }

    // ==================== INITIALIZATION ====================

    function init() {
        hookWebSocket();
        
        // For CloseOption, also try to extract credentials immediately
        if (platform === 'closeoption') {
            const creds = extractCloseOptionCreds();
            if (creds) {
                closeOptionCreds = creds;
                console.log('[CO SSID] Credentials found on load:', creds.substring(0, 30) + '...');
            } else {
                console.log('[CO SSID] Credentials not found yet, will try on menu click');
            }
        }

        // For PocketOption, scan for existing WebSocket instances
        if (platform === 'pocketoption') {
            // Scan immediately
            scanExistingWebSockets();
            
            // Scan again after a delay (in case WebSocket is created later)
            setTimeout(scanExistingWebSockets, 1000);
            setTimeout(scanExistingWebSockets, 3000);
            
            console.log('[PO SSID] Initialization complete, waiting for auth message...');
        }

        console.log(`[SSID] Ready for ${platform}. Use Violentmonkey/Tampermonkey menu to extract credentials.`);
    }

    // Run at document-start to catch early WebSocket creation
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', init);
    } else {
        init();
    }
})();