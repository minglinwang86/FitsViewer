"""Check private proxy credentials without launching or connecting to any server."""
from pathlib import Path
import sys,unittest
from unittest.mock import patch
from importlib.metadata import PackageNotFoundError

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'jupyter'))
from fitsviewer import server

class BackendProxyConfiguration(unittest.TestCase):
    def setUp(self):
        environment=patch.object(server,'process_environment',return_value={'FITSVIEWER_HOME':'owned-fixture'})
        environment.start();self.addCleanup(environment.stop)

    def config(self):
        with (patch.object(server,'resources',return_value=(ROOT,ROOT/'unused-binary',ROOT/'web')),
             patch.object(server,'process_environment',return_value={'FITSVIEWER_HOME':str(ROOT)}),
             patch.object(server,'load_settings',return_value={'browser_port':None,'proxy_port':None})):
            return server.setup_server()

    def test_private_header_and_child_environment_match_without_printable_secret(self):
        config=self.config()
        token=config['environment']()['FITSVIEWER_JUPYTER_TOKEN']
        self.assertGreaterEqual(len(token),32)
        self.assertTrue(config['request_headers_override']()['X-Fitsviewer-Token']==token)
        self.assertTrue(token not in repr(config))
        with (patch.object(server,'resources',return_value=(ROOT,ROOT/'unused-binary',ROOT/'web')),
             patch.object(server,'load_settings',return_value={'browser_port':None,'proxy_port':None})):
            command=config['command'](54321,'/owned-auth-test/')
        self.assertTrue(token not in repr(command))

    def test_independent_proxy_instances_have_independent_credentials(self):
        first=self.config();second=self.config()
        a=first['environment']()['FITSVIEWER_JUPYTER_TOKEN'];b=second['environment']()['FITSVIEWER_JUPYTER_TOKEN']
        self.assertTrue(a!=b)
        self.assertTrue(first['request_headers_override']()['X-Fitsviewer-Token']==a)
        self.assertTrue(second['request_headers_override']()['X-Fitsviewer-Token']==b)

    def test_installed_server_proxy_accepts_real_callable_hooks(self):
        from jupyter_server_proxy.config import ServerProcess
        process=ServerProcess(name='owned-auth-configuration-test',**self.config())
        handler,_=process.make_proxy_handler()
        self.assertIsNotNone(handler)
        self.assertTrue(callable(process.environment))
        self.assertTrue(callable(process.request_headers_override))

    def test_unsupported_proxy_versions_fail_closed_with_actionable_message(self):
        for version in ['3.2.3','4.3.0','5.0.0']:
            with self.subTest(version=version),patch.object(server,'version',return_value=version):
                with self.assertRaisesRegex(RuntimeError,'requires jupyter-server-proxy>=4.4,<5'):
                    self.config()
        with patch.object(server,'version',side_effect=PackageNotFoundError()):
            with self.assertRaisesRegex(RuntimeError,'restart Jupyter'):self.config()

if __name__=='__main__':unittest.main()
