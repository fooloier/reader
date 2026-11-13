import subprocess
from support import ROOT, RuntimeCase, rust_test_count


class DomainTests(RuntimeCase):
    def test_runtime_parser_rules(self):
        result = subprocess.run(["cargo", "test", "--locked", "-p", "reader-runtime", "--lib"], cwd=ROOT, capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(rust_test_count(result.stdout), 7)

    def test_core_builds_and_tests_independently(self):
        result = subprocess.run(["cargo", "test", "--locked", "-p", "reader-core"], cwd=ROOT, capture_output=True, text=True, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(rust_test_count(result.stdout), 2)

    def test_absolute_xdg_and_defaults(self):
        paths = self.command(op="linux_paths", home="/test/home", env={"XDG_DATA_HOME": "/custom/data", "XDG_CONFIG_HOME": "relative", "XDG_CACHE_HOME": "", "XDG_STATE_HOME": "/custom/state"})
        self.assertEqual(paths, {"data": "/custom/data/reader", "config": "/test/home/.config/reader", "cache": "/test/home/.cache/reader", "state": "/custom/state/reader"})
        result = self.commands({"op": "linux_paths", "home": "relative", "env": {}})[0]
        self.assertFalse(result["ok"])
