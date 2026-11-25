#!/usr/bin/env python3
"""Package every Wallet language; a failed language does not stop the others."""
import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'script/lib'))
from cli import normalized_argv
from task_report import Report, add_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    add_options(parser, 'package-examples')
    parser.epilog = 'Shared tools are installed once; failure skips all dependent language packages.'
    args = parser.parse_args(normalized_argv())
    report = Report(args, 'MuduDB example packages')
    report.run('install-tools', ['cargo', 'make', 'install-tools'])
    for language, folder in [('rs', 'wallet'), ('as', 'wallet-as'), ('cs', 'wallet-cs'),
                             ('py', 'wallet-py'), ('go', 'wallet-go'), ('c', 'wallet-c')]:
        report.run(language, ['cargo', 'make', 'package'], ROOT / 'example/mpk' / folder,
                   needs=['install-tools'])
    return report.finish()


if __name__ == '__main__':
    sys.exit(main())
