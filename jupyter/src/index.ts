import { JupyterFrontEnd, JupyterFrontEndPlugin } from '@jupyterlab/application';
import { IDocumentManager } from '@jupyterlab/docmanager';
import { ABCWidgetFactory, DocumentRegistry, DocumentWidget } from '@jupyterlab/docregistry';
import { PageConfig, URLExt } from '@jupyterlab/coreutils';
import { ILauncher } from '@jupyterlab/launcher';
import { ContentsManager, Drive, IContentProvider, ServerConnection } from '@jupyterlab/services';
import { Widget } from '@lumino/widgets';
import { fitsviewerIcon } from './icon';

const PROVIDER = 'fitsviewer-metadata';
const FACTORY = 'FitsViewer';

class FitsContent extends Widget {
  readonly iframe: HTMLIFrameElement;
  constructor(readonly context: DocumentRegistry.Context) {
    super();
    this.addClass('fitsviewer-document');
    this.node.style.position = 'relative';
    this.iframe = document.createElement('iframe');
    this.iframe.dataset.testid = 'fitsviewer-frame';
    Object.assign(this.iframe.style, {position: 'absolute', inset: '0', width: '100%', height: '100%', border: '0'});
    this.node.appendChild(this.iframe);
    this.load();
    context.pathChanged.connect(this.onPathChanged, this);
  }
  private onPathChanged(): void { this.load(); }
  release(): void {
    // Synchronous same-origin cleanup before the iframe is detached.
    // 在 iframe 移除之前同步销毁内部渲染器。
    try { this.iframe.contentWindow?.dispatchEvent(new Event('fitsviewer-dispose')); }
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
    this.iframe.title = `FitsViewer · ${this.context.path}`;
    this.iframe.src = URLExt.join(PageConfig.getBaseUrl(), 'fitsviewer/') + '?' + query;
  }
}

class FitsFactory extends ABCWidgetFactory<DocumentWidget<FitsContent>> {
  created?: (widget: DocumentWidget<FitsContent>) => void;
  protected createNewWidget(context: DocumentRegistry.Context): DocumentWidget<FitsContent> {
    const content = new FitsContent(context);
    void context.ready.then(() => { context.model.readOnly = true; context.model.dirty = false; });
    const widget = new DocumentWidget({content, context});
    widget.title.icon = fitsviewerIcon;
    // The parent's disposed signal precedes DOM detachment; child disposal is later.
    // 父窗口先发 disposed 信号再移除 DOM，必须在这里释放 iframe 内部资源。
    widget.disposed.connect(content.release, content);
    widget.title.caption = 'FitsViewer';
    queueMicrotask(() => { if (!widget.isDisposed) { this.created?.(widget); } });
    return widget;
  }
}

const plugin: JupyterFrontEndPlugin<void> = {
  id: 'fitsviewer:viewer',
  autoStart: true,
  requires: [IDocumentManager],
  optional: [ILauncher],
  activate: async (app: JupyterFrontEnd, documents: IDocumentManager, launcher: ILauncher | null) => {
    const viewers = new Set<DocumentWidget<FitsContent>>();
    const reuseWindow = (): boolean => {
      try { return JSON.parse(localStorage.getItem('fitsviewer-ui-v1') || '{}').open_in_same_window === true; }
      catch { return false; }
    };
    window.addEventListener('message', event => {
      if (event.origin !== location.origin || event.data?.type !== 'fitsviewer-exit-window') { return; }
      if (![...viewers].some(widget => !widget.isDisposed && widget.content.iframe.contentWindow === event.source)) { return; }
      for (const widget of [...viewers]) { widget.close(); }
    });
    const settings = ServerConnection.makeSettings();
    const response = await ServerConnection.makeRequest(URLExt.join(settings.baseUrl, 'fitsviewer-settings'), {method: 'GET'}, settings);
    if (!response.ok && response.status !== 404) { throw new Error('FitsViewer settings could not be loaded'); }
    const preferences = response.ok ? await response.json() as {default_viewer: boolean} : {default_viewer: true};
    // Fetch only file metadata through Contents; FITS bytes go directly to Rust.
    // Contents 只读取元数据，FITS 字节直接交给 Rust 后端。
    const provider: IContentProvider = {
      get: async (path) => {
        const metadata = await app.serviceManager.contents.get(path, {content: false});
        return {...metadata, content: '', format: 'text', mimetype: 'text/plain', writable: false};
      },
      save: async () => { throw new Error('FitsViewer is a read-only viewer'); }
    };
    ((app.serviceManager.contents as ContentsManager).defaultDrive as Drive).contentProviderRegistry.register(PROVIDER, provider);
    app.docRegistry.addFileType({
      name: 'fitsviewer-fits', displayName: 'FITS image',
      extensions: ['.fits', '.fit', '.fts', '.fits.gz', '.fit.gz', '.fts.gz', '.FITS', '.FIT', '.FTS', '.FITS.GZ', '.FIT.GZ', '.FTS.GZ', '.fz', '.fits.fz', '.fit.fz', '.fts.fz', '.FZ', '.FITS.FZ', '.FIT.FZ', '.FTS.FZ'],
      mimeTypes: ['application/fits', 'image/fits'], contentType: 'file', fileFormat: 'text'
    });
    app.docRegistry.addFileType({name:'fitsviewer-spectrum',displayName:'Spectrum data',extensions:['.dat','.txt','.csv','.ascii','.dat.gz','.txt.gz','.csv.gz'],mimeTypes:['text/plain'],contentType:'file',fileFormat:'text'});
    const factory = new FitsFactory({
      name: FACTORY, label: FACTORY, fileTypes: ['fitsviewer-fits','fitsviewer-spectrum','text','csv'], defaultFor: preferences.default_viewer ? ['fitsviewer-fits'] : [],
      modelName: 'text', readOnly: true, contentProviderId: PROVIDER
    });
    factory.created = widget => {
      // Replace only FitsViewer tabs; each replacement keeps its own file context.
      // 只替换查看器标签；不改动旧文件的 Context，不关闭 Notebook 或普通编辑器。
      if (reuseWindow()) { for (const previous of [...viewers]) { previous.close(); } }
      viewers.add(widget);
      widget.disposed.connect(() => { viewers.delete(widget); });
    };
    app.docRegistry.addWidgetFactory(factory);

    // Non-kernel launcher cards use their command's LabIcon, not kernelIconUrl.
    // 普通启动卡片直接使用命令图标，代理只负责按需启动服务。
    const openViewer = 'fitsviewer:open-window';
    app.commands.addCommand(openViewer, {
      label: 'FitsViewer',
      caption: 'Open FitsViewer in a window',
      icon: fitsviewerIcon,
      execute: () => {
        const reuse = reuseWindow();
        const opened = window.open(URLExt.join(PageConfig.getBaseUrl(), 'fitsviewer/'), reuse ? 'FitsViewer' : '_blank', reuse ? '' : 'noopener');
        if (opened) { opened.opener = null; }
      }
    });
    launcher?.add({command: openViewer, category: 'Other'});

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
            const url = URLExt.join(settings.baseUrl, 'fitsviewer/api/requests') + '?after=' + after;
            const response = await ServerConnection.makeRequest(url, {method: 'GET'}, settings);
            if (response.ok) {
              const data = await response.json() as {epoch: string; latest: number; requests: {seq: number; path: string; hdu?: number}[]};
              if (epoch !== data.epoch) {
                epoch = data.epoch;
                after = Number(sessionStorage.getItem('fitsviewer-seq:' + epoch) ?? data.latest);
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
              sessionStorage.setItem('fitsviewer-seq:' + epoch, String(after));
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
