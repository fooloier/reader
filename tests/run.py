import argparse
import pathlib
import subprocess
import sys
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[1]
LAYERS = ("unit", "integration", "ui")


def main():
    parser = argparse.ArgumentParser(description="运行 Reader 分层测试")
    parser.add_argument("layers", nargs="*", metavar="层", help="unit、integration、ui；默认全部")
    parser.add_argument("--skip-build", action="store_true")
    args = parser.parse_args()
    for layer in args.layers:
        if layer not in LAYERS:
            parser.error(f"未知测试层：{layer}")
    if not args.skip_build:
        result = subprocess.run(
            ["cargo", "build", "--locked", "--workspace", "--all-targets", "--features", "test-support"],
            cwd=ROOT,
            check=False,
            timeout=1200,
        )
        if result.returncode:
            return result.returncode
    suite = unittest.TestSuite()
    for layer in args.layers or LAYERS:
        directory = ROOT / "tests" / layer
        if not directory.is_dir():
            parser.error(f"测试层尚未建立：{layer}")
        cases = unittest.defaultTestLoader.discover(
            str(directory), pattern="test_*.py", top_level_dir=str(ROOT / "tests")
        )
        if not cases.countTestCases():
            parser.error(f"测试层没有测试用例：{layer}")
        suite.addTests(cases)
    artifacts = ROOT / "target/test-results"
    artifacts.mkdir(parents=True, exist_ok=True)
    with (artifacts / "layers.log").open("w") as log:
        class Output:
            def write(self, value):
                sys.stderr.write(value)
                log.write(value)

            def flush(self):
                sys.stderr.flush()
                log.flush()

        return 0 if unittest.TextTestRunner(stream=Output(), verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
