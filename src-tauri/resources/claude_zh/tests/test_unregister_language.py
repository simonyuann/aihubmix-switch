import re
import unittest

BASE_LANGUAGE_LIST = '["en-US","de-DE","fr-FR","ko-KR","ja-JP","es-419","es-ES","it-IT","hi-IN","pt-BR","id-ID"'
LANGUAGE_LIST_PATTERN = re.escape(BASE_LANGUAGE_LIST) + r'(?:(?:,"zh-CN")|(?:,"zh-TW")|(?:,"zh-HK"))*\]'
ANCHOR = '"en-US","de-DE","fr-FR","ko-KR","ja-JP"'


def unregister_language_text(text: str) -> tuple[str, bool]:
    """Pure simulation of the corrected Unregister-Language logic."""
    if ANCHOR not in text:
        return text, False
    regex = re.compile(LANGUAGE_LIST_PATTERN)
    updated = regex.sub(BASE_LANGUAGE_LIST + ']', text)
    return updated, updated != text


class TestUnregisterLanguage(unittest.TestCase):
    def test_preserves_native_locale_objects_in_cff2bf6cb_and_shared_14(self):
        # Native snippet from cff2bf6cb-fXcSRqFE.js
        cff2_snippet = '-SP":Jn,"sv-SE":Yn,"tr-TR":Xn,"uk-UA":Zn,"zh-CN":Qn,"zh-TW":$n};'
        updated, changed = unregister_language_text(cff2_snippet)
        self.assertFalse(changed)
        self.assertEqual(updated, cff2_snippet)

        # Native snippet from shared-14-BWwhbyI9.js
        shared14_snippet = 'const list=["de","es","fr","it","ja","ko","pt","ru","zh-CN","zh-TW","id"];'
        updated, changed = unregister_language_text(shared14_snippet)
        self.assertFalse(changed)
        self.assertEqual(updated, shared14_snippet)

    def test_resets_registered_whitelists(self):
        native_noise = ';const map={"zh-CN":1,"zh-TW":2};'
        for suffix in [',"zh-CN"', ',"zh-TW","zh-HK"', ',"zh-CN","zh-TW","zh-CN"', '']:
            input_text = f"const a={BASE_LANGUAGE_LIST}{suffix}];const b={BASE_LANGUAGE_LIST}{suffix}]{native_noise}"
            expected = f"const a={BASE_LANGUAGE_LIST}];const b={BASE_LANGUAGE_LIST}]{native_noise}"

            updated, changed = unregister_language_text(input_text)
            self.assertEqual(updated, expected)

            # Test idempotence
            updated2, changed2 = unregister_language_text(updated)
            self.assertEqual(updated2, expected)
            self.assertFalse(changed2)


if __name__ == "__main__":
    unittest.main()
