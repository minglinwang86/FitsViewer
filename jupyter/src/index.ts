import { JupyterFrontEnd, JupyterFrontEndPlugin } from '@jupyterlab/application';
import { IDocumentManager } from '@jupyterlab/docmanager';
import { ABCWidgetFactory, DocumentRegistry, DocumentWidget } from '@jupyterlab/docregistry';
import { PageConfig, URLExt } from '@jupyterlab/coreutils';
import { ContentsManager, Drive, IContentProvider, ServerConnection } from '@jupyterlab/services';
import { Widget } from '@lumino/widgets';

const PROVIDER = 'fitsview-metadata';
const FACTORY = 'FITSView';

class FitsContent extends Widget {
  readonly iframe: HTMLIFrameElement;
  constructor(readonly context: DocumentRegistry.Context) {
    super();
    this.addClass('fitsview-document');
    this.node.style.position = 'relative';
    this.iframe = document.createElement('iframe');
    this.iframe.dataset.testid = 'fitsview-frame';
    Object.assign(this.iframe.style, {position: 'absolute', inset: '0', width: '100%', height: '100%', border: '0'});
    this.node.appendChild(this.iframe);
    this.load();
    context.pathChanged.connect(this.onPathChanged, this);
  }
  private onPathChanged(): void { this.load(); }
  release(): void {
    // Synchronous same-origin cleanup before the iframe is detached.
    // 在 iframe 移除之前同步销毁内部渲染器。
    try { this.iframe.contentWindow?.dispatchEvent(new Event('fitsview-dispose')); }
    catch { /* An already unloaded frame needs no message. / 已卸载的页无需通知。 */ }
  }
  dispose(): void {
    if (this.isDisposed) { return; }
    this.context.pathChanged.disconnect(this.onPathChanged, this);
    this.release();
    this.iframe.src = 'about:blank';
    this.iframe.remove();
    super.dispose();
  }
  load(hdu?: number): void {
    if (this.isDisposed) { return; }
    this.release();
    const query = new URLSearchParams({path: this.context.path});
    if (hdu !== undefined) { query.set('hdu', String(hdu)); }
    this.iframe.title = `FITSView · ${this.context.path}`;
    this.iframe.src = URLExt.join(PageConfig.getBaseUrl(), 'fitsview/') + '?' + query;
  }
}

class FitsFactory extends ABCWidgetFactory<DocumentWidget<FitsContent>> {
  protected createNewWidget(context: DocumentRegistry.Context): DocumentWidget<FitsContent> {
    const content = new FitsContent(context);
    void context.ready.then(() => { context.model.readOnly = true; context.model.dirty = false; });
    const widget = new DocumentWidget({content, context});
    // The parent's disposed signal precedes DOM detachment; child disposal is later.
    // 父窗口先发 disposed 信号再移除 DOM，必须在这里释放 iframe 内部资源。
    widget.disposed.connect(content.release, content);
    widget.title.caption = 'FITSView';
    return widget;
  }
}

const plugin: JupyterFrontEndPlugin<void> = {
  id: 'fitsview-jupyter:viewer',
  autoStart: true,
  requires: [IDocumentManager],
  activate: async (app: JupyterFrontEnd, documents: IDocumentManager) => {
    const settings = ServerConnection.makeSettings();
    const response = await ServerConnection.makeRequest(URLExt.join(settings.baseUrl, 'fitsview-settings'), {method: 'GET'}, settings);
    if (!response.ok && response.status !== 404) { throw new Error('FITSView settings could not be loaded'); }
    const preferences = response.ok ? await response.json() as {default_viewer: boolean} : {default_viewer: true};
    // Fetch only file metadata through Contents; FITS bytes go directly to Rust.
    // Contents 只读取元数据，FITS 字节直接交给 Rust 后端。
    const provider: IContentProvider = {
      get: async (path) => {
        const metadata = await app.serviceManager.contents.get(path, {content: false});
        return {...metadata, content: '', format: 'text', mimetype: 'text/plain', writable: false};
      },
      save: async () => { throw new Error('FITSView is a read-only viewer'); }
    };
    ((app.serviceManager.contents as ContentsManager).defaultDrive as Drive).contentProviderRegistry.register(PROVIDER, provider);
    app.docRegistry.addFileType({
      name: 'fitsview-fits', displayName: 'FITS image',
      extensions: ['.fits', '.fit', '.fts', '.fits.gz', '.fit.gz', '.fts.gz', '.FITS', '.FIT', '.FTS', '.FITS.GZ', '.fz', '.fits.fz'],
      mimeTypes: ['application/fits', 'image/fits'], contentType: 'file', fileFormat: 'text'
    });
    app.docRegistry.addWidgetFactory(new FitsFactory({
      name: FACTORY, label: FACTORY, fileTypes: ['fitsview-fits'], defaultFor: preferences.default_viewer ? ['fitsview-fits'] : [],
      modelName: 'text', readOnly: true, contentProviderId: PROVIDER
    }));

    // CLI requests open a document in the connected Lab window. A user click on
    // "Open in window" creates a popup without bypassing browser popup policies.
    // CLI 请求在当前 Lab 中打开标签页；独立窗口由用户点击按钮打开。
    void app.restored.then(() => {
      const settings = ServerConnection.makeSettings();
      let epoch = '';
      let after = 0;
      const poll = async (): Promise<void> => {
        try {
          if (document.visibilityState === 'visible') {
            const url = URLExt.join(settings.baseUrl, 'fitsview/api/requests') + '?after=' + after;
            const response = await ServerConnection.makeRequest(url, {method: 'GET'}, settings);
            if (response.ok) {
              const data = await response.json() as {epoch: string; latest: number; requests: {seq: number; path: string; hdu?: number}[]};
              if (epoch !== data.epoch) {
                epoch = data.epoch;
                after = Number(sessionStorage.getItem('fitsview-seq:' + epoch) ?? data.latest);
              }
              for (const request of data.requests.slice(-1)) {
                if (request.seq <= after) { continue; }
                const widget = documents.openOrReveal(request.path, FACTORY);
                if (widget && request.hdu !== undefined && request.hdu !== null) {
                  (widget as DocumentWidget<FitsContent>).content.load(request.hdu);
                }
                after = request.seq;
              }
              after = Math.max(after, data.latest);
              sessionStorage.setItem('fitsview-seq:' + epoch, String(after));
            }
          }
        } catch { /* A stopped viewer is retried on the next poll. / 下一轮重试。 */ }
        window.setTimeout(() => { void poll(); }, 2000);
      };
      void poll();
    });
  }
};

export default plugin;
