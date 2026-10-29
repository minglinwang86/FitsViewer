import UIKit
import WebKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        let window = UIWindow(frame: UIScreen.main.bounds)
        window.rootViewController = ViewerController()
        window.makeKeyAndVisible()
        self.window = window
        return true
    }
}

final class ViewerController: UIViewController, WKNavigationDelegate {
    private var viewer: WKWebView!
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .default()
        viewer = WKWebView(frame: .zero, configuration: configuration)
        viewer.navigationDelegate = self
        viewer.translatesAutoresizingMaskIntoConstraints = false
        viewer.scrollView.isScrollEnabled = false
        view.addSubview(viewer)
        NSLayoutConstraint.activate([
            viewer.leadingAnchor.constraint(equalTo: view.leadingAnchor), viewer.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            viewer.topAnchor.constraint(equalTo: view.topAnchor), viewer.bottomAnchor.constraint(equalTo: view.bottomAnchor)
        ])
        guard let index = Bundle.main.url(forResource: "index", withExtension: "html", subdirectory: "site") else { return }
        viewer.loadFileURL(index, allowingReadAccessTo: index.deletingLastPathComponent())
        // The HTML file input invokes WebKit's system picker. File bytes stay on device.
    }
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard let url = navigationAction.request.url else { decisionHandler(.cancel); return }
        decisionHandler(url.isFileURL && url.path.hasPrefix(Bundle.main.bundlePath + "/site/") ? .allow : .cancel)
    }
}
