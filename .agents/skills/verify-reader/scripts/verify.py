#!/usr/bin/env python3
import datetime
import fcntl
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[4]

def main():
    mode = sys.argv[1] if len(sys.argv) == 2 else ''
    if mode == 'doctor':
        for path in ('Cargo.lock', 'tests/ui/test_production.py', 'target/debug/reader', 'target/debug/reader-driver'):
            if not (ROOT / path).is_file():
                raise SystemExit(f'缺少 {path}，先执行 Launch')
        subprocess.run(['cargo', 'metadata', '--locked', '--offline', '--no-deps', '--format-version=1'], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        print('源码与本地构建产物可用；构建新鲜度以本轮 Launch 成功为准。')
        return
    if mode not in ('ui', 'native', 'integration'):
        raise SystemExit('用法：verify.py doctor|ui|native|integration')
    artifacts = ROOT / 'target/test-results'
    artifacts.mkdir(parents=True, exist_ok=True)
    with (artifacts / 'verification.lock').open('w') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        evidence = ROOT / 'target/verification' / (datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f') + '-' + mode)
        evidence.mkdir(parents=True)
        command = ['python3', 'tests/native_smoke.py'] if mode == 'native' else ['python3', 'tests/run.py', mode, '--skip-build']
        (evidence / 'command.txt').write_text(' '.join(command) + '\n')
        (evidence / 'revision.txt').write_bytes(subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT))
        (evidence / 'ui-source.rs').write_bytes((ROOT / 'src/ui_tests.rs').read_bytes())
        started = datetime.datetime.now().timestamp()
        try:
            with (evidence / 'transcript.log').open('w') as log:
                result = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT)
            (evidence / 'exit-code.txt').write_text(str(result.returncode) + '\n')
        finally:
            for path in artifacts.iterdir():
                if path.is_file() and path.name != 'verification.lock' and path.stat().st_mtime >= started:
                    (evidence / path.name).write_bytes(path.read_bytes())
            print(evidence)
        raise SystemExit(result.returncode)

if __name__ == '__main__':
    main()
