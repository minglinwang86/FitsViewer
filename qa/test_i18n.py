"""Check translation key coverage without translating scientific identifiers."""
from collections import Counter
from pathlib import Path
import json
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]
LOCALES = ROOT / 'web/locales'
def unique_object(pairs):
    result={}
    for key,value in pairs:
        if key in result:raise ValueError('Duplicate translation key: '+key)
        result[key]=value
    return result
def load(name):return json.loads((LOCALES/name).read_text(encoding='utf-8'),object_pairs_hook=unique_object)


class TranslationCoverage(unittest.TestCase):
    def test_registry_and_embedded_sources_cover_six_locales(self):
        registry=load('registry.json')
        self.assertEqual(registry['default'],'en')
        codes=[entry['code'] for entry in registry['languages']]
        self.assertEqual(codes,['en','zh-CN','it','es','de','fr'])
        rust=(ROOT/'crates/viewer/src/i18n.rs').read_text(encoding='utf-8')
        for entry in registry['languages']:
            self.assertEqual(entry['file'],entry['code']+'.json')
            self.assertTrue(entry['name'].strip() and entry['label'].strip())
            self.assertIn('include_str!("../../../web/locales/'+entry['file']+'")',rust)

    def test_every_locale_has_complete_unique_nonempty_keys(self):
        english=load('en.json')
        self.assertGreater(len(english),900)
        self.assertTrue(all(key==value for key,value in english.items()))
        for entry in load('registry.json')['languages']:
            messages=load(entry['file'])
            with self.subTest(locale=entry['code']):
                self.assertEqual(set(messages),set(english))
                self.assertTrue(all(isinstance(value,str) and value.strip() for value in messages.values()))

    def test_interpolation_numbers_and_latin_prefix_spaces_are_preserved(self):
        placeholders=lambda value:Counter(re.findall(r'\{[A-Za-z_]\w*\}',value))
        numbers=lambda value:Counter(re.findall(r'(?<![A-Za-z])\d+(?:\.\d+)?',value))
        for entry in load('registry.json')['languages']:
            for key,value in load(entry['file']).items():
                with self.subTest(locale=entry['code'],key=key):
                    self.assertEqual(placeholders(key),placeholders(value))
                    number_text=value
                    if entry['code']=='zh-CN':
                        number_text=number_text.replace('二维','2D')
                        reviewed={
                            '1:1':'原始大小',
                            'Preview textures are limited to 1 Mi pixels per layer; measurements use the active original image.':'每层显示纹理最多约一百万像素；测量使用当前原始图像。',
                        }
                        if key in reviewed:
                            self.assertEqual(value,reviewed[key])
                            number_text=key  # Existing, explicitly reviewed semantic number labels.
                    self.assertFalse(numbers(key)-numbers(number_text))
                    # The existing Chinese dictionary intentionally omits interword prefix spaces.
                    # 中文既有拼接不强加拉丁语空格；迁移保留原有译文。
                    if entry['code']!='zh-CN':
                        self.assertEqual(re.match(r'^\s*',key)[0],re.match(r'^\s*',value)[0])
                        self.assertEqual(re.search(r'\s*$',key)[0],re.search(r'\s*$',value)[0])

    def test_shell_uses_the_shared_dictionary_and_all_literal_keys_exist(self):
        html=(ROOT/'web/index.html').read_text(encoding='utf-8')
        self.assertNotIn('const pageTranslations=',html)
        self.assertIn("from './locales/index.js'",html)
        english=load('en.json')
        keys=re.findall(r"pageText\('([^']*)'",html)
        self.assertGreater(len(keys),10)
        self.assertTrue(all(key in english for key in keys))


if __name__ == "__main__":
    unittest.main()
