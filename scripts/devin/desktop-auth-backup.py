#!/usr/bin/env python3
"""Encrypted, same-profile Devin Desktop auth snapshots. Never copies chat databases."""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import pwd
import sqlite3
import subprocess
import sys
import tempfile

AUTH_KEYS = (
    'windsurfAuthStatus', 'codeium.windsurf-windsurf_auth',
    'secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.sessions"}',
    'secret://{"extensionId":"codeium.windsurf","key":"windsurf_auth.apiServerUrl"}',
)
FIELDS = ('apiServerUrl', 'lastLoginEmail')


def identity(profile):
    state = profile / 'Local State'
    return {'profile': str(profile.resolve()), 'uid': os.getuid(),
            'local_state_sha256': hashlib.sha256(state.read_bytes()).hexdigest()}


def snapshot(db):
    records = {}
    blob_keys = []
    for key in AUTH_KEYS:
        row = db.execute('SELECT value FROM ItemTable WHERE key=?', (key,)).fetchone()
        if row is not None and isinstance(row[0], bytes):
            blob_keys.append(key)
        records[key] = None if row is None else base64.b64encode(
            row[0] if isinstance(row[0], bytes) else row[0].encode()).decode()
    row = db.execute('SELECT value FROM ItemTable WHERE key=?', ('codeium.windsurf',)).fetchone()
    state = json.loads(row[0]) if row else {}
    return {'records': records, 'fields': {k: state.get(k) for k in FIELDS},
            'blob_keys': blob_keys}


def validate(data):
    # Legacy snapshots omitted blob_keys and stored all values as text.
    if not isinstance(data, dict) or set(data) not in (
            {'records', 'fields'}, {'records', 'fields', 'blob_keys'}):
        raise ValueError('备份字段不符合登录快照格式')
    if (not isinstance(data['records'], dict) or set(data['records']) != set(AUTH_KEYS)
            or not isinstance(data['fields'], dict) or set(data['fields']) != set(FIELDS)):
        raise ValueError('备份字段不符合登录快照格式')
    blobs = data.get('blob_keys', [])
    if (not isinstance(blobs, list) or any(not isinstance(k, str) for k in blobs)
            or len(set(blobs)) != len(blobs) or not set(blobs).issubset(AUTH_KEYS)
            or any(data['records'][k] is None for k in blobs)):
        raise ValueError('登录字段存储类型错误')
    for key, value in data['records'].items():
        if value is not None:
            if not isinstance(value, str):
                raise ValueError('登录字段编码错误')
            try:
                decoded = base64.b64decode(value, validate=True)
                if key not in blobs:
                    decoded.decode('utf-8')
            except (ValueError, UnicodeError) as error:
                raise ValueError('登录字段编码错误') from error
    for value in data['fields'].values():
        if value is not None and not isinstance(value, str):
            raise ValueError('账号字段类型错误')


def restore(db, data):
    validate(data)
    row = db.execute('SELECT value FROM ItemTable WHERE key=?', ('codeium.windsurf',)).fetchone()
    state = json.loads(row[0]) if row else {}
    if not isinstance(state, dict):
        raise ValueError('当前 Desktop 设置格式错误')
    with db:
        for key, value in data['records'].items():
            if value is None:
                db.execute('DELETE FROM ItemTable WHERE key=?', (key,))
            else:
                db.execute('INSERT OR REPLACE INTO ItemTable(key,value) VALUES (?,?)',
                           (key, base64.b64decode(value) if key in data.get('blob_keys', [])
                            else base64.b64decode(value).decode()))
        for key, value in data['fields'].items():
            if value is None:
                state.pop(key, None)
            else:
                state[key] = value
        db.execute('INSERT OR REPLACE INTO ItemTable(key,value) VALUES (?,?)',
                   ('codeium.windsurf', json.dumps(state)))


def require_closed():
    # Fail closed if any Desktop process for this user remains, including helpers.
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != os.getuid():
                continue
            executable = os.readlink(entry / 'exe').lower()
            if 'devin-desktop' in executable or '/windsurf/' in executable:
                raise ValueError('请先正常退出 Devin Desktop，再执行恢复；工具不会终止应用')
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue


def encrypt(path, payload):
    # GPG obtains the passphrase through pinentry; never argv/environment/files.
    fd, temporary = tempfile.mkstemp(prefix='.devin-auth-', dir=path.parent)
    os.close(fd)
    try:
        subprocess.run(['gpg', '--symmetric', '--cipher-algo', 'AES256', '--yes',
                        '--output', temporary], input=json.dumps(payload).encode(), check=True)
        # Exclusive hard-link publication prevents accidental overwrite of backups.
        os.link(temporary, path)
    finally:
        os.unlink(temporary)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['backup', 'restore'])
    parser.add_argument('file', type=Path)
    parser.add_argument('--profile', type=Path, default=Path(pwd.getpwuid(os.getuid()).pw_dir) / '.config/Devin')
    args = parser.parse_args()
    profile = args.profile.resolve()
    database = profile / 'User/globalStorage/state.vscdb'
    marker = identity(profile)
    if args.action == 'backup':
        with sqlite3.connect(database.as_uri() + '?mode=ro', uri=True) as db:
            db.execute('BEGIN')
            data = snapshot(db)
        encrypt(args.file, {'version': 1, 'client_type': 'desktop', 'identity': marker, 'auth': data})
        print('已生成加密 Desktop 登录快照；未验证服务端登录有效性')
    else:
        raw = subprocess.run(['gpg', '--decrypt', str(args.file)], capture_output=True, check=True).stdout
        payload = json.loads(raw)
        if payload.get('version') != 1 or payload.get('client_type') != 'desktop' or payload.get('identity') != marker:
            raise ValueError('仅支持原用户、原配置目录、相同 Local State 的恢复；跨机迁移尚未支持')
        validate(payload['auth'])
        require_closed()
        with sqlite3.connect(database.as_uri() + '?mode=rw', uri=True) as db:
            db.execute('BEGIN')
            original = snapshot(db)
            db.rollback()
            rollback = args.file.with_name(args.file.name + '.before-restore-' + __import__('uuid').uuid4().hex + '.gpg')
            encrypt(rollback, {'version': 1, 'client_type': 'desktop', 'identity': marker, 'auth': original})
            require_closed()
            db.execute('BEGIN IMMEDIATE')
            if identity(profile) != marker or snapshot(db) != original:
                raise ValueError('备份期间登录数据发生变化，已取消恢复，请重试')
            restore(db, payload['auth'])
        print('登录字段已恢复。回退快照：' + str(rollback))
        print('请启动 Desktop 核对账号并测试请求；此结果不代表登录验收成功')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Do not expose decrypted records or subprocess output.
        print('操作失败：' + (str(error) if isinstance(error, ValueError) else type(error).__name__), file=sys.stderr)
        sys.exit(1)
