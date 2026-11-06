// Minimal streamable-HTTP MCP client for the relay's /s/{token}/mcp link.
let nextId = 1;

async function readJson(res) {
  const text = await res.text();
  if ((res.headers.get('content-type') || '').includes('text/event-stream')) {
    const data = text.split('\n').filter((l) => l.startsWith('data:')).map((l) => l.slice(5).trim()).filter(Boolean);
    return data.length ? JSON.parse(data[data.length - 1]) : null;
  }
  return text ? JSON.parse(text) : null;
}

export async function connectMcp(origin, token) {
  const url = `${origin}/s/${token}/mcp`;
  const headers = { 'content-type': 'application/json', accept: 'application/json, text/event-stream' };
  const post = (body, extra = {}) => fetch(url, { method: 'POST', headers: { ...headers, ...extra }, body: JSON.stringify(body) });
  const init = await post({ jsonrpc: '2.0', id: nextId++, method: 'initialize', params: { protocolVersion: '2025-03-26', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } } });
  if (!init.ok) throw new Error(`mcp initialize: ${init.status} ${await init.text()}`);
  const session = init.headers.get('mcp-session-id');
  await init.text();
  const sess = session ? { 'mcp-session-id': session } : {};
  await post({ jsonrpc: '2.0', method: 'notifications/initialized' }, sess);

  /** Calls a tool; returns the parsed JSON of its text content (or the raw text). */
  async function call(name, args = {}) {
    const res = await post({ jsonrpc: '2.0', id: nextId++, method: 'tools/call', params: { name, arguments: args } }, sess);
    const msg = await readJson(res);
    if (msg?.error) throw new Error(`${name}: ${JSON.stringify(msg.error)}`);
    const r = msg.result;
    const text = (r.content || []).filter((c) => c.type === 'text').map((c) => c.text).join('\n');
    if (r.isError) throw new Error(`${name}: ${text}`);
    try { return JSON.parse(text); } catch { return text; }
  }
  return { call, url };
}

/** Depth-first search of a JSON value for the first object matching `pred`. */
export function findDeep(v, pred) {
  if (v && typeof v === 'object') {
    if (!Array.isArray(v) && pred(v)) return v;
    for (const c of Object.values(v)) { const f = findDeep(c, pred); if (f) return f; }
  }
  return null;
}
