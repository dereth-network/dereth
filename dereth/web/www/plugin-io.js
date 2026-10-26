// What the client's plugins reach outside the page, as globals the WebAssembly module calls: the
// bus between this site's clients in other tabs, the plugins' HTTPS requests, and the world's
// plugin policy document.

// The bus: a broadcast channel reaches every other tab of this site and not this one.
const peerInbox = [];
let peerChannel = null;

// Plugins' HTTPS requests, with the browser's fetch; each ended one is handed over as a JSON line
// and the body's bytes.
const netEnded = [];

// The world's plugin policy document, fetched at start when the page was given its https URL.
let pluginPolicy = '';

export function installPluginIo() {
  peerChannel = new BroadcastChannel('dereth-plugin-peers');
  peerChannel.onmessage = (e) => {
    if (typeof e.data === 'string' && peerInbox.length < 1024) peerInbox.push(e.data);
  };
  globalThis.derethPeersSend = (text) => peerChannel.postMessage(text);
  globalThis.derethPeersTake = () => JSON.stringify(peerInbox.splice(0));
  globalThis.derethNetStart = (id, json) => {
    const r = JSON.parse(json);
    fetch(r.url, { method: r.method, headers: r.headers, body: r.body ?? undefined, redirect: 'error' })
      .then(async (res) => {
        const body = new Uint8Array(await res.arrayBuffer());
        netEnded.push({ id, status: res.status, type: res.headers.get('content-type'), body });
      })
      .catch((e) => netEnded.push({ id, error: String(e) }));
  };
  globalThis.derethNetNext = () => {
    const d = netEnded.shift();
    if (!d) return new Uint8Array(0);
    const head = new TextEncoder().encode(
      JSON.stringify({ id: d.id, status: d.status, type: d.type, error: d.error }) + '\n');
    const out = new Uint8Array(head.length + (d.body ? d.body.length : 0));
    out.set(head);
    if (d.body) out.set(d.body, head.length);
    return out;
  };
  globalThis.derethPluginPolicy = () => pluginPolicy;
}

// Fetch the policy document at `url` (the page's `?plugin_policy=`), an https URL; anything else,
// or a failed fetch, leaves the world with no published policy.
export async function fetchPluginPolicy(url) {
  pluginPolicy = '';
  if (typeof url === 'string' && url.startsWith('https://')) {
    pluginPolicy = await fetch(url)
      .then((r) => (r.ok ? r.text() : ''))
      .catch(() => '');
  }
}
