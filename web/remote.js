// One application-only fetch boundary; credentials remain in memory.
// 仅拦截本应用 API；仅图像请求使用取消器，超时覆盖响应体读取而不止响应头。
export function imageFetchBoundary(base, credential, originalFetch = window.fetch.bind(window)) {
  const root = new URL(base), prefix = root.pathname.replace(/\/$/, '') + '/api/';
  const pending = new Set(), closedGroups = new Set();
  function keep(group, keys) {
    if (typeof group !== 'string' || !Array.isArray(keys)) return;
    const wanted = new Set(keys);
    if (!keys.length) {
      closedGroups.add(group);
      if (closedGroups.size > 128) closedGroups.delete(closedGroups.values().next().value);
    }
    for (const request of pending) {
      if (request.group === group && (!keys.length || request.kind === 'tile' && !wanted.has(request.key))) request.controller.abort();
    }
  }
  async function fetchImage(input, options = {}) {
    let url = new URL(typeof input === 'string' || input instanceof URL ? input : input.url, root);
    if (url.origin !== root.origin || !url.pathname.startsWith(prefix)) return originalFetch(input, options);
    const originalUrl = url.href;
    const isRequest = typeof Request !== 'undefined' && input instanceof Request;
    const headers = new Headers(typeof input === 'object' && input.headers ? input.headers : undefined);
    new Headers(options.headers).forEach((value, key) => headers.set(key, value));
    const token = credential();
    if (token) {
      headers.set('X-Fitsviewer-Token', token);
      // Existing artifact URLs from WASM may include its token. Strip before network/history.
      if (url.searchParams.has('token')) url.searchParams.delete('token');
    }
    // Preserve Request's internal byte body: reconstructing it with a different URL
    // turns that body into a streaming upload, which Chromium rejects over HTTP/1.
    // 保留常规 POST 的原始 Request；仅确需去掉 URL 令牌时有界缓冲请求体。
    let requestInput = isRequest ? input : url;
    if (isRequest && url.href !== originalUrl) {
      const method = options.method || input.method;
      const init = {method, mode: input.mode, credentials: input.credentials, cache: input.cache,
        redirect: input.redirect, referrer: input.referrer, referrerPolicy: input.referrerPolicy,
        integrity: input.integrity, keepalive: input.keepalive, signal: input.signal};
      if (!['GET', 'HEAD'].includes(method.toUpperCase()) && input.body && options.body === undefined) {
        const reader = input.clone().body.getReader(), chunks = [];
        let length = 0;
        try {
          for (;;) {
            const {value, done} = await reader.read();
            if (done) break;
            length += value.byteLength;
            if (length > 8 * 1024 * 1024) throw new RangeError('Viewer request body exceeds 8 MiB');
            chunks.push(value);
          }
        } catch (error) { reader.cancel(error).catch(() => {}); throw error; }
        const bytes = new Uint8Array(length);
        let offset = 0;
        for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
        init.body = bytes;
      }
      requestInput = url;
      options = {...init, ...options};
    }
    const kind = url.pathname.slice(prefix.length);
    if (!/^image(?:\/(?:metadata|full|tile|pixel))?$/.test(kind) || options.keepalive) return originalFetch(requestInput, {...options, headers});
    const controller = new AbortController(), group = url.searchParams.get('fv_image');
    const request = {controller, group, kind: kind.split('/')[1] || 'full', key: ['level', 'x', 'y'].map(key => url.searchParams.get(key) || '0').join(':')};
    const signal = options.signal || (typeof input === 'object' ? input.signal : undefined);
    const abort = () => controller.abort();
    signal?.addEventListener('abort', abort, {once: true});
    const timer = setTimeout(abort, ['metadata', 'full'].includes(request.kind) ? 60000 : 20000);
    const cleanup = () => { clearTimeout(timer); pending.delete(request); signal?.removeEventListener('abort', abort); };
    controller.signal.addEventListener('abort', cleanup, {once: true});
    pending.add(request);
    if (signal?.aborted || group && closedGroups.has(group)) controller.abort();
    try {
      const response = await originalFetch(requestInput, {...options, headers, signal: controller.signal});
      if (!response.body) { cleanup(); return response; }
      const reader = response.body.getReader();
      const body = new ReadableStream({
        async pull(stream) {
          try { const result = await reader.read(); if (result.done) { cleanup(); stream.close(); } else stream.enqueue(result.value); }
          catch (error) { cleanup(); stream.error(error); }
        },
        cancel(reason) { controller.abort(); cleanup(); return reader.cancel(reason); }
      });
      const wrapped = new Response(body, {status: response.status, statusText: response.statusText, headers: response.headers});
      Object.defineProperty(wrapped, 'url', {value: response.url});
      return wrapped;
    } catch (error) { cleanup(); throw error; }
  }
  return {fetch: fetchImage, keep, dispose() { for (const request of pending) request.controller.abort(); pending.clear(); closedGroups.clear(); }};
}

export function remoteSession(base, bootstrapTicket, ui) {
  let credential = '', instance = '', target = '', fixed = false, downloadOnly = false, current, pending, timer, stopped = false, failures = 0;
  const status = (text, fallback) => ui.status(text, fallback);
  async function api(route, data, keepalive = false) {
    const headers = {'Content-Type': 'application/json'};
    if (credential) headers['X-Fitsviewer-Token'] = credential;
    const controller = new AbortController(), timeout = setTimeout(() => controller.abort(), 5000);
    try {
      const response = await fetch(new URL('api/fver/' + route, base), {headers, method: data === undefined ? 'GET' : 'POST', body: data === undefined ? undefined : JSON.stringify(data), signal: controller.signal, keepalive, cache: 'no-store', credentials: 'omit', referrerPolicy: 'no-referrer'});
      if (!response.ok) {const error = new Error(await response.text()); error.status = response.status; throw error;}
      const value = await response.json();
      if (instance && value.instance && value.instance !== instance) throw new Error('Server identity changed; run fver again');
      return value;
    } finally { clearTimeout(timeout); }
  }
  async function acknowledge(request, action, keepalive = false) {
    return api('ack', {request_id: request.request_id, attempt_epoch: request.attempt_epoch, action}, keepalive);
  }
  async function open(request, alreadyAccepted = false) {
    if (!alreadyAccepted) request = (await acknowledge(request, 'claim')).request;
    current = request; pending = request;
    status(`Target ${target}: accepted; waiting for first frame`);
    if (ui.started()) await ui.open(request);
    await acknowledge(request, 'visible');
    return request;
  }
  async function tick() {
    if (stopped) return;
    try {
      const event = await api('events');
      failures = 0;
      const own = current && event.requests.find(request => request.request_id === current.request_id);
      if (own && (own.attempt_epoch !== current.attempt_epoch || ['cancelled', 'expired', 'superseded', 'closed'].includes(own.state))) {
        pending = null;
        if (ui.started()) await ui.close();
        status(`Target ${target}: ${own.state}; file permission ended`);
        current = null;
      }
      if (pending && ui.started()) {
        const state = ui.state();
        if (state?.path === pending.path && state.first_frame_ready) {
          await acknowledge(pending, 'first_frame');
          pending = null;
          status(`Target ${target}: first frame displayed`);
        } else if (state?.path === pending.path && state.error && !state.loading) {
          await acknowledge(pending, 'failed'); pending = null;
          status(`Target ${target}: unable to display file: ${state.error}`);
        }
      }
      const queued = event.requests.filter(request => request.state === 'queued' && request.route === 'browser').at(-1);
      if (queued && !fixed && ui.started() && !ui.state()?.loading) await open(queued);
      const remote = event.requests.filter(request => request.route === 'helper' && ['accepted', 'visible', 'unknown', 'first_frame'].includes(request.state)).at(-1);
      if (remote && !fixed && (!current || remote.seq > current.seq)) {
        status(`Target ${target}: helper ${remote.state}${remote.state === 'first_frame' ? ' (displayed)' : '; HTML is available'}`, async () => {
          const result = await acknowledge(remote, 'fallback'); await open(result.request);
        });
      }
    } catch (error) {
      if ([401,403].includes(error.status)) {stopped = true;pending = null;if(ui.started())await ui.close();status(`Viewing permission ended: ${error}. Run fver again for a fresh link.`);return;}
      failures += 1;
      status(`Connection interrupted (${failures}/6): ${error}. ${failures >= 6 ? 'Run fver again for a fresh link.' : 'Retrying…'}`);
    }
    if (!stopped && failures < 6) timer = setTimeout(tick, failures ? Math.min(8000, 500 * 2 ** failures) : 1000);
  }
  return {
    token: () => credential,
    async bootstrap() {
      const result = await api('bootstrap', {ticket: bootstrapTicket});
      credential = result.token; instance = result.instance; target = result.target; fixed = result.fixed;
      if(result.download_artifact){downloadOnly=true;ui.artifact(result.download_artifact,()=>api('release-download',{}));return {downloadOnly:true};}
      const request = result.request;
      if (fixed || request.route === 'browser' && request.state === 'queued') return open(request, fixed);
      if (request.route === 'browser' && ['accepted', 'visible', 'unknown', 'first_frame'].includes(request.state)) return open(request, true);
      status(`Target ${target}: waiting for paired helper; HTML will follow if it cannot accept`);
      return null;
    },
    follow() { if(!downloadOnly)tick(); },
    async openWindow() {
      if (!current) throw new Error('No active browser request');
      const popup = window.open('about:blank', '_blank', 'popup,width=1200,height=850,resizable=yes');
      if (!popup) throw new Error('Allow this site to open a viewer window');
      popup.opener = null;
      try { const link = await api('browser-link', {request_id: current.request_id, attempt_epoch: current.attempt_epoch}); popup.location.replace(base + '#open=' + link.ticket); }
      catch (error) { popup.close(); throw error; }
    },
    dispose() { stopped = true; clearTimeout(timer); if(downloadOnly)api('release-download',{},true).catch(()=>{});else if (current) acknowledge(current, 'close', true).catch(() => {}); }
  };
}
