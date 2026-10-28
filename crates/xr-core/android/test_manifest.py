#!/usr/bin/env python3
"""Host-only tests for reusable Android manifest rendering."""
import importlib.util
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("manifest", HERE / "render-manifest.py")
MANIFEST = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MANIFEST)
ANDROID = "{http://schemas.android.com/apk/res/android}"


class ManifestTests(unittest.TestCase):
    def test_independent_app_identity_and_xml_escaping(self):
        values = dict(APP_PACKAGE="org.example.player", APP_LABEL='Video & "3D"',
                      APP_LIB_NAME="video_player", APP_ACTIVITY="android.app.NativeActivity",
                      APP_HAS_CODE="false")
        xml = MANIFEST.render((HERE / "AndroidManifest.xml.in").read_text(), values)
        root = ET.fromstring(xml)
        self.assertEqual(root.get("package"), values["APP_PACKAGE"])
        app = root.find("application")
        self.assertEqual(app.get(ANDROID + "label"), values["APP_LABEL"])
        self.assertEqual(app.get(ANDROID + "hasCode"), "false")
        activity = app.find("activity")
        self.assertEqual(activity.get(ANDROID + "name"), values["APP_ACTIVITY"])
        library = next(m for m in activity.findall("meta-data")
                       if m.get(ANDROID + "name") == "android.app.lib_name")
        self.assertEqual(library.get(ANDROID + "value"), "video_player")
        self.assertNotIn("@APP_", xml)
        self.assertNotIn("linuxquest", xml)

    def test_missing_setting_fails_instead_of_emitting_a_broken_manifest(self):
        with self.assertRaises(KeyError):
            MANIFEST.render('<manifest package="@APP_PACKAGE@"/>', {})


if __name__ == "__main__":
    unittest.main()
