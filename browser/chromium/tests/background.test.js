"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

class MockEvent {
    listeners = [];

    addListener(listener) {
        this.listeners.push(listener);
    }

    emit(...args) {
        for (const listener of this.listeners) listener(...args);
    }
}

function makeExtension(activeWindow) {
    let now = 1_000;
    let idleState = "active";
    const messages = [];
    const intervals = [];
    const port = {
        onMessage: new MockEvent(),
        onDisconnect: new MockEvent(),
        postMessage: (message) => messages.push(message)
    };
    const chrome = {
        runtime: {
            lastError: null,
            onStartup: new MockEvent(),
            onInstalled: new MockEvent(),
            connectNative: () => port
        },
        windows: {
            WINDOW_ID_NONE: -1,
            onFocusChanged: new MockEvent(),
            getLastFocused: (_options, callback) => callback(activeWindow)
        },
        tabs: {
            onActivated: new MockEvent(),
            onUpdated: new MockEvent(),
            onRemoved: new MockEvent()
        },
        idle: {
            onStateChanged: new MockEvent(),
            queryState: (_seconds, callback) => callback(idleState)
        }
    };
    const context = {
        chrome,
        URL,
        performance: { now: () => now },
        setInterval: (callback) => {
            intervals.push(callback);
            return intervals.length;
        },
        clearInterval: () => {},
        setTimeout: (callback) => callback()
    };
    const source = fs.readFileSync(
        path.join(__dirname, "..", "background.js"),
        "utf8"
    );
    vm.runInNewContext(source + "\nglobalThis.sampleForTest = sampleActiveSite;", context);
    return {
        context,
        chrome,
        messages,
        setTime: (value) => { now = value; },
        setIdle: (value) => { idleState = value; },
        sample: () => context.sampleForTest()
    };
}

async function settle() {
    await new Promise((resolve) => setImmediate(resolve));
}

test("attributes active hostnames across tabs and excludes idle, unfocused, and private windows", async () => {
    const window = {
        focused: true,
        incognito: false,
        state: "normal",
        tabs: [{
            active: true,
            incognito: false,
            url: "https://News.Example.com/article?token=private"
        }]
    };
    const app = makeExtension(window);
    await settle();

    app.setTime(6_000);
    await app.sample();
    await settle();
    assert.deepEqual(
        app.messages.map(({ domain, seconds }) => ({ domain, seconds })),
        [{ domain: "news.example.com", seconds: 5 }]
    );
    assert.equal(JSON.stringify(app.messages).includes("token"), false);

    window.tabs = [{
        active: true,
        incognito: false,
        url: "https://docs.example.org/guide"
    }];
    app.setTime(6_500);
    app.chrome.tabs.onActivated.emit({ tabId: 2, windowId: 1 });
    await settle();
    app.setTime(11_500);
    await app.sample();
    await settle();
    assert.deepEqual(
        app.messages.map(({ domain }) => domain),
        ["news.example.com", "docs.example.org"]
    );

    window.incognito = true;
    window.tabs[0].incognito = true;
    window.tabs[0].url = "https://private.example.net/";
    app.setTime(12_000);
    app.chrome.tabs.onActivated.emit({ tabId: 3, windowId: 2 });
    await settle();
    app.setTime(17_000);
    await app.sample();
    await settle();
    assert.equal(app.messages.some(({ domain }) => domain === "private.example.net"), false);
    assert.equal(app.messages.length, 2);

    window.incognito = false;
    window.tabs[0].incognito = false;
    window.focused = false;
    app.setTime(18_000);
    app.chrome.windows.onFocusChanged.emit(-1);
    await settle();
    app.setTime(23_000);
    await app.sample();
    await settle();
    assert.equal(app.messages.length, 2);

    window.focused = true;
    app.setIdle("idle");
    app.setTime(24_000);
    app.chrome.idle.onStateChanged.emit("idle");
    await settle();
    app.setTime(29_000);
    await app.sample();
    await settle();
    assert.equal(app.messages.length, 2);
});
