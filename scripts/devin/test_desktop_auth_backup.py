import importlib.util
from pathlib import Path
import sqlite3
import unittest
import json
import base64
import copy

spec = importlib.util.spec_from_file_location('backup', Path(__file__).with_name('desktop-auth-backup.py'))
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


class SnapshotTests(unittest.TestCase):
    def test_round_trip_preserves_unrelated_data_and_missing_keys(self):
        db = sqlite3.connect(':memory:')
        db.execute('CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB)')
        db.executemany('INSERT INTO ItemTable VALUES (?,?)', [
            (backup.AUTH_KEYS[0], '{"apiKey":"test-old"}'),
            ('codeium.windsurf', json.dumps({'lastLoginEmail': 'old@example.test', 'theme': 'old'})),
            ('chat.history', 'keep-chat'), ('secret://unrelated', 'keep-secret')])
        old = backup.snapshot(db)
        db.execute('UPDATE ItemTable SET value=? WHERE key=?', ('{"apiKey":"test-new"}', backup.AUTH_KEYS[0]))
        db.execute('INSERT INTO ItemTable VALUES (?,?)', (backup.AUTH_KEYS[2], 'new-session'))
        db.execute('UPDATE ItemTable SET value=? WHERE key=?',
                   (json.dumps({'lastLoginEmail': 'new@example.test', 'theme': 'new', 'newSetting': True}), 'codeium.windsurf'))
        db.commit()
        backup.restore(db, old)
        self.assertEqual(backup.snapshot(db), old)
        state = json.loads(db.execute('SELECT value FROM ItemTable WHERE key="codeium.windsurf"').fetchone()[0])
        self.assertEqual(state['theme'], 'new')
        self.assertTrue(state['newSetting'])
        self.assertEqual(db.execute('SELECT value FROM ItemTable WHERE key="chat.history"').fetchone()[0], 'keep-chat')
        self.assertEqual(db.execute('SELECT value FROM ItemTable WHERE key="secret://unrelated"').fetchone()[0], 'keep-secret')

    def test_binary_and_text_storage_types_survive_roundtrip(self):
        db = sqlite3.connect(':memory:')
        db.execute('CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB)')
        values = [(backup.AUTH_KEYS[0], b'\x00\xffbinary'),
                  (backup.AUTH_KEYS[1], 'text-session')]
        db.executemany('INSERT INTO ItemTable VALUES (?,?)', values)
        saved = backup.snapshot(db)
        db.execute('DELETE FROM ItemTable')
        backup.restore(db, saved)
        for key, value in values:
            restored = db.execute('SELECT value FROM ItemTable WHERE key=?', (key,)).fetchone()[0]
            self.assertEqual(restored, value)
            self.assertIs(type(restored), type(value))

    def test_legacy_snapshot_and_invalid_payloads(self):
        db = sqlite3.connect(':memory:')
        db.execute('CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB)')
        original = backup.snapshot(db)
        legacy = copy.deepcopy(original)
        del legacy['blob_keys']
        legacy['records'][backup.AUTH_KEYS[0]] = base64.b64encode(b'legacy-text').decode()
        backup.restore(db, legacy)
        self.assertEqual(db.execute('SELECT value FROM ItemTable WHERE key=?',
                                    (backup.AUTH_KEYS[0],)).fetchone()[0], 'legacy-text')
        baseline = backup.snapshot(db)
        invalid = [None, [], {'records': None, 'fields': {}}]
        for blobs in [['unknown'], [backup.AUTH_KEYS[0]] * 2, [7], 'text']:
            bad = copy.deepcopy(original)
            bad['blob_keys'] = blobs
            invalid.append(bad)
        for value in [7, 'not base64!', '/w==']:
            bad = copy.deepcopy(original)
            bad['records'][backup.AUTH_KEYS[0]] = value
            invalid.append(bad)
        for bad in invalid:
            with self.subTest(payload=bad):
                with self.assertRaises(ValueError):
                    backup.restore(db, bad)
                self.assertEqual(backup.snapshot(db), baseline)

    def test_rejects_extra_fields_before_writing(self):
        with self.assertRaises(ValueError):
            backup.validate({'records': {'chat.history': 'eA=='}, 'fields': {}})


if __name__ == '__main__':
    unittest.main()
