"use strict";

const HOST_NAME = "com.digitalwellbeing.browser_bridge";
const SAMPLE_INTERVAL_MS = 5_000;
const MAX_CREDIT_SECONDS = 15;

let nativePort = null;
let sampleTimer = null;
let sampleInProgress = false;
let samplePending = false;
let previousSampleAt = 0;
let previousDomain = null;
let retryTimer = null;

function connectNativeHost() {
    if (nativePort) return;
    try {
        nativePort = chrome.runtime.connectNative(HOST_NAME);
    } catch (_) {
        scheduleReconnect();
        return;
    }
    nativePort.onMessage.addListener(() => {});
    nativePort.onDisconnect.addListener(() => {
        // Read the error inside the callback so Chromium does not report an
        // unchecked-runtime-error banner while the app is being updated.
        void chrome.runtime.lastError;
        nativePort = null;
        stopSampling();
        scheduleReconnect();
    });
    previousSampleAt = performance.now();
    previousDomain = null;
    void sampleActiveSite();
    sampleTimer = setInterval(() => void sampleActiveSite(), SAMPLE_INTERVAL_MS);
}

function scheduleReconnect() {
    if (!retryTimer) {
        retryTimer = setTimeout(() => {
            retryTimer = null;
            connectNativeHost();
        }, 10_000);
    }
}

function stopSampling() {
    if (sampleTimer) clearInterval(sampleTimer);
    sampleTimer = null;
    samplePending = false;
    previousDomain = null;
    previousSampleAt = 0;
}

function requestSampleSoon() {
    if (!nativePort) {
        connectNativeHost();
        return;
    }
    if (sampleInProgress) {
        samplePending = true;
        return;
    }
    void sampleActiveSite();
}

function hostnameFromTab(tab) {
    if (!tab || tab.incognito || typeof tab.url !== "string") return null;
    try {
        const parsed = new URL(tab.url);
        if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
        const hostname = parsed.hostname.toLowerCase().replace(/^www\./, "");
        return hostname && hostname.length <= 253 ? hostname : null;
    } catch (_) {
        return null;
    }
}

function sendElapsedSite(now) {
    const elapsedSeconds = Math.floor((now - previousSampleAt) / 1_000);
    if (previousDomain && elapsedSeconds > 0 && nativePort) {
        nativePort.postMessage({
            type: "website_usage",
            domain: previousDomain,
            seconds: Math.min(elapsedSeconds, MAX_CREDIT_SECONDS)
        });
    }
    previousSampleAt = now;
}

function activeDomain() {
    return new Promise((resolve) => {
        chrome.windows.getLastFocused({ populate: true }, (win) => {
            if (chrome.runtime.lastError || !win || !win.focused || win.incognito || win.state === "minimized") {
                resolve(null);
                return;
            }
            chrome.idle.queryState(60, (state) => {
                if (chrome.runtime.lastError || state !== "active") {
                    resolve(null);
                    return;
                }
                const tab = win.tabs && win.tabs.find((candidate) => candidate.active);
                resolve(hostnameFromTab(tab));
            });
        });
    });
}

async function sampleActiveSite() {
    if (!nativePort) return;
    if (sampleInProgress) {
        samplePending = true;
        return;
    }
    sampleInProgress = true;
    try {
        const now = performance.now();
        sendElapsedSite(now);
        previousDomain = await activeDomain();
    } catch (_) {
        previousDomain = null;
    } finally {
        sampleInProgress = false;
        if (samplePending) {
            samplePending = false;
            void sampleActiveSite();
        }
    }
}

chrome.runtime.onStartup.addListener(connectNativeHost);
chrome.runtime.onInstalled.addListener(connectNativeHost);
chrome.windows.onFocusChanged.addListener(requestSampleSoon);
chrome.tabs.onActivated.addListener(requestSampleSoon);
chrome.tabs.onUpdated.addListener((_tabId, changeInfo) => {
    if (changeInfo.url || changeInfo.status === "complete") requestSampleSoon();
});
chrome.tabs.onRemoved.addListener(requestSampleSoon);
chrome.idle.onStateChanged.addListener(requestSampleSoon);
connectNativeHost();
