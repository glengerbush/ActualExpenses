#!/usr/bin/env python3
"""Run all source checks in balanced, isolated CI shards; never skip a check."""
import argparse
import json
import os
from pathlib import Path
import shlex
import subprocess
import time

ROOT = Path(__file__).resolve().parents[1]


def source_commands(root=ROOT):
    scripts = json.loads((root / 'actualBudget/web/package.json').read_text())['scripts']
    commands = []
    for command in json.loads((root / 'tools/qualification-source.json').read_text()):
        if command[:4] == ['npm', '--prefix', 'actualBudget/web', 'run']:
            for part in scripts[command[4]].split(' && '):
                commands.append({'cwd': 'actualBudget/web', 'argv': shlex.split(part)})
        else:
            commands.append({'cwd': '.', 'argv': command})
    return commands


def label(command):
    return shlex.join(command['argv'])


def shards(commands, count, timings):
    if count < 1 or count > len(commands):
        raise ValueError('Shard count must be between 1 and the number of commands')
    groups = [[] for _ in range(count)]
    totals = [0.0] * count
    def weight(command):
        name = label(command)
        if name in timings:
            return max(0.01, timings[name])
        # Conservative estimates for source checks without hosted timing yet.
        if 'generate-v5-receipts' in name: return 35.0
        if 'complete-migration' in name: return 30.0
        if 'monte-carlo' in name: return 15.0
        if 'rule-context' in name: return 15.0
        return 5.0
    for command in sorted(commands, key=lambda item: (-weight(item), label(item))):
        index = min(range(count), key=lambda i: (totals[i], i))
        groups[index].append(command)
        totals[index] += weight(command)
    return groups


def select_shard(commands, index, count):
    if not 0 <= index < count:
        raise ValueError('Shard index must be zero-based and smaller than shard count')
    timings = json.loads((ROOT / 'tools/qualification-timings.json').read_text())
    return shards(commands, count, timings)[index]


def run_commands(commands, root, environment, report, prepare=None):
    results = []
    for command in commands:
        name = label(command)
        print(f'RUN: {name}', flush=True)
        started = time.monotonic()
        command_environment = {**environment, **(prepare() if prepare else {})}
        result = subprocess.run(command['argv'], cwd=root / command['cwd'], env=command_environment)
        elapsed = round(time.monotonic() - started, 3)
        results.append({'command': name, 'seconds': elapsed, 'exit_code': result.returncode})
        print(f'{"PASS" if result.returncode == 0 else "FAIL"}: {name} ({elapsed}s)', flush=True)
    report.parent.mkdir(parents=True, exist_ok=True)
    report.write_text(json.dumps(results, indent=2) + '\n')
    failures = [result for result in results if result['exit_code']]
    print(f'Qualification: {len(results) - len(failures)}/{len(results)} passed', flush=True)
    if failures:
        raise RuntimeError(f'Failed commands: {[result["command"] for result in failures]}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shard-index', type=int, required=True)
    parser.add_argument('--shard-count', type=int, required=True)
    parser.add_argument('--list', action='store_true')
    args = parser.parse_args()
    commands = select_shard(source_commands(), args.shard_index, args.shard_count)
    if args.list:
        print(json.dumps(commands, indent=2))
    else:
        run_commands(commands, ROOT, os.environ.copy(),
                     ROOT / f'qualification-results/source-{args.shard_index}.json')


if __name__ == '__main__':
    main()
