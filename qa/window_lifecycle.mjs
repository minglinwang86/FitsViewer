// Execute the actual browser window-control definitions with a mocked renderer.
// 检查实际页面脚本的退出时序与跨窗口归属，不将 mock 结果当成完整浏览器验收。
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const html = fs.readFileSync(new URL('../web/index.html', import.meta.url), 'utf8').replaceAll('\r\n', '\n');
let source = html.match(/<script type="module">([\s\S]*?)<\/script>/)[1];
source = source.replace(/^import .*;\s*$/gm, '');
source = source.slice(0, source.indexOf('try{\n const query=')) + '\nstarted=true;';
const pending = [];
const channels = new Map();
function viewer(base, embedded = false) {
  const state = {stops: 0, closes: 0, messages: [], rendering: false, requests: 0};
  const node = {style: {}, addEventListener() {}, replaceChildren() {}, getContext() { return {getExtension() { return {loseContext() {}}; }}; }};
  const window = {location: new URL(base), addEventListener() {}, open() {}, fetch() { state.requests++; }};
  window.parent = embedded ? {postMessage(message, origin) { state.messages.push({message, origin}); }} : window;
  window.close = () => { state.closes++; };
  class Channel {
    constructor(name) { this.name = name; this.closed = false; channels.set(name, [...(channels.get(name) || []), this]); }
    postMessage(data) { for (const other of channels.get(this.name)) { if (other !== this && !other.closed) { pending.push(() => { if (!other.closed) { other.onmessage?.({data}); } }); } } }
    close() { this.closed = true; }
  }
  const sandbox = {
    window, location: window.location, URL, URLSearchParams, BroadcastChannel: Channel,
    crypto: {randomUUID: () => 'test'}, console, Blob,
    document: {documentElement: {}, cookie: '', createElement() { return {style: {}, append() {}, click() {}}; }, getElementById() { return node; }},
    setTimeout(callback) { pending.push(callback); return pending.length; }, clearTimeout() {},
    imageFetchBoundary: () => ({fetch() {}, dispose() {}, keep() {}}), remoteSession() {},
    createLocalization: () => ({text: key => key, setLanguage: async () => 'en', language: 'en', ready: Promise.resolve()}),
    localStorage: {getItem() { return '{}'; }},
    stop() { assert.equal(state.rendering, false, 'WebRunner cannot be stopped inside its active render callback'); state.stops++; }, command() {}
  };
  vm.runInNewContext(source, sandbox, {filename: 'FitsViewer-window-definitions.js'});
  return {window, state};
}
const first = viewer('http://127.0.0.1:8765/');
const second = viewer('http://127.0.0.1:8765/?path=other.fits');
const embedded = viewer('http://127.0.0.1:8765/?path=embedded.fits', true);
const unrelated = viewer('http://127.0.0.1:9999/');
first.state.rendering = true;
first.window.FitsViewer.exitAll();
assert.equal(first.state.stops, 0);
first.state.rendering = false;
while (pending.length) { pending.shift()(); }
for (const view of [first, second, embedded]) { assert.equal(view.state.stops, 1); assert.equal(view.state.requests, 0); }
assert.equal(first.state.closes, 1);
assert.equal(second.state.closes, 1);
assert.equal(embedded.state.closes, 0);
assert.equal(embedded.state.messages[0].message.type, 'fitsviewer-exit-window');
assert.equal(embedded.state.messages[0].origin, 'http://127.0.0.1:8765');
assert.equal(unrelated.state.stops, 0);
assert.equal(unrelated.state.closes, 0);
console.log('Browser window lifecycle: deferred teardown, scoped broadcast, embedded close request and no server shutdown passed');
