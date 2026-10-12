"""Synchronize the shared header-only C ABI loader into the Go package."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
def sync():
    source = ROOT / 'crates/sdk/bindings/c/mududb/adapter/native.h'
    target = ROOT / 'crates/sdk/bindings/go/adapter/native.h'
    target.write_text('/* Generated from mududb/adapter/native.h by script/sdk/sync_native.py. Do not edit manually. */\n' + source.read_text(), encoding='utf-8')
if __name__ == '__main__':
    sync()
