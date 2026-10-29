package org.fitsviewer.mini;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.webkit.*;
import android.widget.FrameLayout;
import androidx.webkit.WebViewAssetLoader;

public final class MainActivity extends Activity {
    private static final int PICK_FILE = 17;
    private WebView webView;
    private ValueCallback<Uri[]> picker;

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        webView = new WebView(this);
        WebView.setWebContentsDebuggingEnabled((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0);
        FrameLayout content = new FrameLayout(this);
        content.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(), insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets.consumeSystemWindowInsets();
        });
        content.addView(webView, new FrameLayout.LayoutParams(-1, -1));
        setContentView(content);
        WebSettings settings = webView.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(false);
        settings.setAllowContentAccess(true); // Only the user's document-picker grant.
        settings.setAllowFileAccessFromFileURLs(false);
        settings.setAllowUniversalAccessFromFileURLs(false);
        settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
        WebViewAssetLoader loader = new WebViewAssetLoader.Builder()
                .addPathHandler("/assets/", new WebViewAssetLoader.AssetsPathHandler(this)).build();
        webView.setWebViewClient(new WebViewClient() {
            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                WebResourceResponse local = loader.shouldInterceptRequest(request.getUrl());
                if (local != null) return local;
                return new WebResourceResponse("text/plain", "UTF-8", 403, "Offline app", null,
                        new java.io.ByteArrayInputStream(new byte[0]));
            }
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri url = request.getUrl();
                return !("https".equals(url.getScheme()) && "appassets.androidplatform.net".equals(url.getHost()) && url.getPath().startsWith("/assets/"));
            }
        });
        webView.setWebChromeClient(new WebChromeClient() {
            @Override public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                if (picker != null) picker.onReceiveValue(null);
                picker = callback;
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE)
                        .setType("*/*").addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                try { startActivityForResult(intent, PICK_FILE); }
                catch (android.content.ActivityNotFoundException error) { picker.onReceiveValue(null); picker = null; }
                return true;
            }
        });
        webView.loadUrl("https://appassets.androidplatform.net/assets/index.html");
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == PICK_FILE && picker != null) {
            picker.onReceiveValue(result == RESULT_OK && data != null && data.getData() != null ? new Uri[]{data.getData()} : null);
            picker = null;
        }
    }
    @Override protected void onPause() { webView.onPause(); super.onPause(); }
    @Override protected void onResume() { super.onResume(); if (webView != null) webView.onResume(); }
    @Override protected void onDestroy() {
        if (picker != null) { picker.onReceiveValue(null); picker = null; }
        webView.destroy(); super.onDestroy();
    }
}
