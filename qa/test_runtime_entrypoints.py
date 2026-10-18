"""CLI side-effect and session round-trip regressions; no services or core are run."""
import importlib
import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
sys.dont_write_bytecode=True

class RuntimeEntrypoints(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.name='fitsviewer_runtime_entry_test'
        spec=importlib.util.spec_from_file_location(cls.name,ROOT/'jupyter/fitsviewer/__init__.py',submodule_search_locations=[str(ROOT/'jupyter/fitsviewer')])
        module=importlib.util.module_from_spec(spec);sys.modules[cls.name]=module;spec.loader.exec_module(module)
        cls.cli=importlib.import_module(cls.name+'.cli');cls.scientific=importlib.import_module(cls.name+'.scientific')
    @classmethod
    def tearDownClass(cls):
        for name in list(sys.modules):
            if name.startswith(cls.name):sys.modules.pop(name)
    def test_headless_flags_never_warm_a_jupyter_service(self):
        for flag in ['--packet','--inspect-json','--header-json','--measure-json','--validate-view','--build-info']:
            with self.subTest(flag=flag),patch.object(sys,'argv',['fitsviewer',flag,'science.fits']),patch.object(self.cli,'resources',return_value=(ROOT,ROOT/'bin/fitsviewer',ROOT/'web')),patch.object(self.cli,'load_settings',return_value={'viewer_port':0}),patch.object(self.cli,'process_environment',return_value={}),patch.object(self.cli,'_warm_jupyter') as warm,patch.object(self.cli.subprocess,'call',return_value=0) as call:
                with self.assertRaises(SystemExit) as error:self.cli.main()
                self.assertEqual(error.exception.code,0);warm.assert_not_called();call.assert_called_once()
    def test_session_keeps_selection_link_and_comparison_view(self):
        value={'schema':1,'mode':'PRO','selected':1,'secondary':0,'link':'Pixel','views':[{'path':'a.fits','hdu':0,'view':{}},{'path':'b.fits','hdu':1,'view':{'load_options':{'error_hdu':2,'error_kind':'variance'}},'comparison_view':{'zoom':2.,'pan':[14.,-2.],'fit':False}}]}
        def core(flag,path=None,value=None):return {'dataset':{'hdu':0}} if flag=='--inspect-json' else value
        with patch.object(self.scientific,'binary_json',side_effect=core):
            result=self.scientific.validate_session(value)
        self.assertEqual((result['selected'],result['secondary'],result['link']),(1,0,'Pixel'))
        self.assertEqual(result['views'][1]['comparison_view'],value['views'][1]['comparison_view'])
        self.assertEqual(result['views'][1]['view']['load_options']['error_hdu'],2)
    def test_invalid_comparison_view_is_rejected(self):
        value={'schema':1,'views':[{'path':'a.fits','view':{},'comparison_view':{'zoom':float('inf')}}]}
        with patch.object(self.scientific,'binary_json',return_value={'dataset':{'hdu':0}}),self.assertRaisesRegex(ValueError,'zoom'):
            self.scientific.validate_session(value)

if __name__=='__main__':unittest.main(verbosity=2)
