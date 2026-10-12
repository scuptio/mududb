"""Standalone export and native ABI regression tests."""
import ctypes
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from sdk_build import CONFIG, ROOT, export

class WalletExportTests(unittest.TestCase):
    def test_all_languages_have_two_entries_and_bilingual_guides(self):
        with tempfile.TemporaryDirectory() as temporary:
            for lang in CONFIG:
                with self.subTest(language=lang):
                    repo=Path(temporary)/lang
                    export(lang,repo,'test-commit')
                    wallet=repo/'examples/wallet'
                    self.assertTrue((wallet/'mpm.toml').is_file())
                    self.assertTrue((wallet/'mpk/sql/ddl.sql').is_file())
                    self.assertTrue(any((wallet/'adapter').iterdir()))
                    for name in ('README.md','README.zh-CN.md'):
                        self.assertTrue((repo/name).is_file())
                        text=(wallet/name).read_text(encoding='utf-8')
                        self.assertIn('MUDU_ADAPTER_LIBRARY',text)
                        self.assertIn('mpm build',text)
                    self.assertFalse(any(p.is_symlink() for p in repo.rglob('*')))
                    if lang == 'go':
                        # Keep staged CLI sources out of the SDK's `go test ./...`.
                        self.assertTrue((wallet/'adapter/go.mod').is_file())
                    if lang == 'cs':
                        # WASI compiler packages require the experimental feed.
                        self.assertIn('dotnet-experimental', (wallet/'mpk/nuget.config').read_text())

    def test_go_native_header_matches_single_source(self):
        source=(ROOT/'crates/sdk/bindings/c/mududb/adapter/native.h').read_text()
        generated=(ROOT/'crates/sdk/bindings/go/adapter/native.h').read_text()
        self.assertEqual(generated.split('\n',1)[1],source)

@unittest.skipUnless(os.environ.get('MUDU_ADAPTER_LIBRARY'),'set MUDU_ADAPTER_LIBRARY to exercise the real runtime')
class NativeAdapterTests(unittest.TestCase):
    def test_sql_roundtrip_and_failure(self):
        sys.path.insert(0,str(ROOT/'crates/sdk/bindings/python'))
        from mududb.adapter import use_adapter
        from mududb.db import Database
        from mududb.sql import Params,SqlStmt
        from mududb.result import as_i64
        with tempfile.TemporaryDirectory() as temporary:
            previous=os.environ.get('MUDU_CONNECTION')
            os.environ['MUDU_CONNECTION']='sqlite://'+str(Path(temporary)/'test.db')
            try:
                transport=use_adapter()
                db=Database.open()
                try:
                    db.command(SqlStmt('CREATE TABLE sdk_abi_test (id BIGINT PRIMARY KEY, balance BIGINT)'))
                    self.assertEqual(db.command(SqlStmt('INSERT INTO sdk_abi_test VALUES (?, ?)'),Params().bind(0,1).bind(1,900)),1)
                    result=db.query(SqlStmt('SELECT balance FROM sdk_abi_test WHERE id = ?'),Params().bind(0,1))
                    self.assertTrue(result.next());self.assertEqual(as_i64(result.current_row().value(0)),900)
                    self.assertFalse(result.next())
                    with self.assertRaises(Exception):db.command(SqlStmt('INSERT INTO missing_table VALUES (1)'))
                    for _ in range(20):
                        with self.assertRaises(RuntimeError):transport.call('unknown',b'')
                        with self.assertRaises(RuntimeError):transport.call('query',b'invalid frame')
                finally:db.close()
            finally:
                if previous is None:os.environ.pop('MUDU_CONNECTION',None)
                else:os.environ['MUDU_CONNECTION']=previous

if __name__=='__main__':unittest.main()
