"""Offline protocol behavior: no network, real terminal or service writes."""
import argparse
import hashlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
from contextlib import redirect_stdout

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = ROOT / '.codex/skills/webclx-terminal-message/scripts'
sys.path.insert(0, str(SCRIPTS))
import collaboration_protocol as protocol
import message_status

def args(**overrides):
    values = dict(message='继续核对现有证据', message_file=None, evidence=[], authority_source=None, authority_ref=None,
                  kind='task', decision=None, wake_key=None, check_kind='hourly', wake_action='ensure', next_cycle=False, dry_run=False)
    return SimpleNamespace(**{**values, **overrides})

class ProtocolTests(unittest.TestCase):
    def test_short_unicode_limit_and_no_truncation(self):
        normalize = lambda value, label:' '.join(value.split())
        self.assertEqual(protocol.prepare_message(args(message='中'*300), normalize), '中'*300)
        with self.assertRaisesRegex(RuntimeError, '301'):
            protocol.prepare_message(args(message='中'*301), normalize)

    def test_both_cli_variants_reject_long_body_before_network(self):
        for index, path in enumerate([SCRIPTS/'send_terminal_message.py',ROOT/'builtin-skills/webclx-terminal-message/scripts/send_terminal_message.py']):
            spec=importlib.util.spec_from_file_location('sender_variant_'+str(index),path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
            entry=getattr(module,'main',None) or module.run
            with patch.object(sys,'argv',[str(path),'--target','s1','--message','中'*301]), patch.object(module,'api_json') as api:
                with self.assertRaisesRegex(RuntimeError,'maximum is 300'):entry()
                api.assert_not_called()

    def test_message_file_sender_queues_literal_once(self):
        with tempfile.TemporaryDirectory() as root:
            file=Path(root)/'body.txt';file.write_text('literal `tick` $(do-not-execute)')
            path=SCRIPTS/'send_terminal_message.py';spec=importlib.util.spec_from_file_location('file_sender',path);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
            with patch.object(sys,'argv',[str(path),'--target','worker','--message-file',str(file),'--authority-source','manager','--authority-ref','task-plan']),patch.object(module,'resolve_sender_session',return_value={'id':'manager','name':'manager','group_role':'manager'}),patch.object(module,'resolve_target_session',return_value={'id':'worker','group_role':'executor'}),patch.object(module,'api_json',return_value={'ok':True,'task':{'id':'t','due_at':5000,'status':'queued'}}) as api,redirect_stdout(io.StringIO()) as output:
                module.main()
                api.assert_called_once();self.assertIn('$(do-not-execute)',api.call_args.args[2]['text']);self.assertIn('authority=manager:task-plan',api.call_args.args[2]['text']);self.assertFalse(json.loads(output.getvalue())['submitted'])

    def test_message_file_is_literal_and_evidence_not_loaded(self):
        with tempfile.TemporaryDirectory() as root:
            source = Path(root)/'summary.txt';source.write_text('保留 `x` 与 $(command)\n继续')
            result = protocol.prepare_message(args(message=None, message_file=str(source), evidence=['/missing/report.md']), lambda value,label:' '.join(value.split()))
            self.assertIn('$(command)', result);self.assertIn('/missing/report.md', result)

    def test_sources_and_escalations_require_references(self):
        for values in [{'authority_source':'user'}, {'authority_ref':'contract'}, {'kind':'escalation'}]:
            with self.assertRaises(RuntimeError):protocol.prepare_message(args(**values), lambda value,label:value)

    def test_manager_progress_does_not_reach_supervisor(self):
        manager={'id':'m','group_role':'普通管理员'};supervisor={'id':'s','group_role':'超级管理员'}
        with self.assertRaises(RuntimeError):protocol.validate_route(args(kind='progress'), manager, supervisor)
        protocol.validate_route(args(kind='completed'), manager, supervisor)
        protocol.validate_route(args(kind='escalation',decision='选A或B'), manager, supervisor)
        with self.assertRaises(RuntimeError):protocol.validate_route(args(kind='completed'), {'id':'e','group_role':'executor'}, supervisor)
        with self.assertRaises(RuntimeError):protocol.validate_route(args(kind='completed'), None, supervisor)

    def test_self_wake_requires_real_current_identity(self):
        with patch.dict(os.environ, {'WEBCLX_TERMINAL_ID':'s1'}, clear=True):
            protocol.validate_route(args(wake_key='p'), {'id':'s1'}, {'id':'s1'})
            with self.assertRaises(RuntimeError):protocol.validate_route(args(wake_key='p'), {'id':'s2'}, {'id':'s2'})

    def run_wake(self, api, root, **changes):
        return protocol.queue_wakeup(api, 'http://localhost:11111', {'session_id':'s1','text':'check','due_at':5000}, args(wake_key='p',**changes),root)

    def test_ensure_reuses_and_update_patches_same_task(self):
        with tempfile.TemporaryDirectory() as root:
            rows=[];calls=[]
            def api(base,path,payload=None,method=None):
                calls.append((path,payload,method))
                if payload is None:return {'tasks':rows.copy()}
                if method=='PATCH':rows[0].update(payload);return {'ok':True,'task':rows[0].copy()}
                rows.append({'id':'t1','status':'queued','session_id':'s1','label':payload['label'],'due_at':6000});return {'ok':True,'task':rows[0].copy()}
            self.run_wake(api,root);self.assertTrue(self.run_wake(api,root)['reused'])
            self.run_wake(api,root,wake_action='update')
            self.assertEqual(sum(payload is not None and method is None for _,payload,method in calls),1)
            self.assertEqual(calls[-1][0],'/api/terminal/scheduled-inputs/t1');self.assertEqual(calls[-1][2],'PATCH')

    def test_uncertain_post_never_retries_when_missing(self):
        with tempfile.TemporaryDirectory() as root:
            posts=[]
            def api(base,path,payload=None,method=None):
                if payload is None:return {'tasks':[]}
                posts.append(payload);raise TimeoutError('unknown')
            with self.assertRaises(TimeoutError):self.run_wake(api,root)
            with self.assertRaisesRegex(RuntimeError,'prior wake absent'):self.run_wake(api,root)
            self.assertEqual(len(posts),1)

    def test_uncertain_next_cycle_does_not_retry_old_confirmed(self):
        with tempfile.TemporaryDirectory() as root:
            label=protocol.wake_label('http://localhost:11111','s1','p','hourly');posts=[]
            def api(base,path,payload=None,method=None):
                if payload is None:return {'tasks':[{'id':'old','session_id':'s1','label':label,'status':'confirmed','due_at':1000}]}
                posts.append(payload);raise TimeoutError('unknown')
            with self.assertRaises(TimeoutError):self.run_wake(api,root,next_cycle=True)
            with self.assertRaisesRegex(RuntimeError,'POST outcome unknown'):self.run_wake(api,root,next_cycle=True)
            self.assertEqual(len(posts),1)

    def test_uncertain_post_reconciles_late_server_task(self):
        with tempfile.TemporaryDirectory() as root:
            rows=[];posts=[]
            def api(base,path,payload=None,method=None):
                if payload is None:return {'tasks':rows.copy()}
                posts.append(payload);rows.append({'id':'late','session_id':'s1','label':payload['label'],'status':'queued','due_at':6000});raise TimeoutError()
            with self.assertRaises(TimeoutError):self.run_wake(api,root)
            self.assertEqual(self.run_wake(api,root)['task']['id'],'late');self.assertEqual(len(posts),1)

    def test_dispatching_cannot_be_updated(self):
        with tempfile.TemporaryDirectory() as root:
            label=protocol.wake_label('http://localhost:11111','s1','p','hourly')
            def api(base,path,payload=None,method=None):
                self.assertIsNone(payload);return {'tasks':[{'id':'busy','session_id':'s1','label':label,'status':'dispatching'}]}
            with self.assertRaisesRegex(RuntimeError,'only a queued'):self.run_wake(api,root,wake_action='update')

    def test_concurrent_ensure_creates_one_task(self):
        with tempfile.TemporaryDirectory() as root:
            rows=[];posts=[];errors=[]
            def api(base,path,payload=None,method=None):
                if payload is None:return {'tasks':rows.copy()}
                posts.append(payload);rows.append({'id':'one','session_id':'s1','label':payload['label'],'status':'queued','due_at':6000});return {'ok':True,'task':rows[0].copy()}
            def worker():
                try:self.run_wake(api,root)
                except Exception as error:errors.append(error)
            threads=[threading.Thread(target=worker) for _ in range(4)]
            for thread in threads:thread.start()
            for thread in threads:thread.join(3);self.assertFalse(thread.is_alive())
            self.assertFalse(errors);self.assertEqual(len(posts),1)

    def test_draft_confirmation_does_not_claim_restoration(self):
        result=message_status.inspect_task({'id':'t','session_id':'s','status':'confirmed','last_error':'restore verify; parked=buf:sha256=1234567890abcdef'})
        self.assertEqual(result['delivery_status'],'confirmed');self.assertEqual(result['draft_restore_status'],'unverified_archive_retained')
        self.assertEqual(result['business_ack'],'not_evaluated')

    def test_draft_export_verifies_hash_preserves_archive_and_private_file(self):
        with tempfile.TemporaryDirectory() as root:
            source=Path(root)/'archive.json';destination=Path(root)/'draft.txt';text='private original';digest=hashlib.sha256(text.encode()).hexdigest()
            source.write_text(json.dumps({'entries':[{'session_id':'s','task_id':'t','text':text,'digest':digest,'buffer':'buf'}]}));before=source.read_bytes()
            task={'id':'t','session_id':'s','status':'confirmed','parked_draft_ref':'parked=buf:sha256='+digest[:16]}
            result=message_status.inspect_task(task,source,destination)
            self.assertTrue(result['archive_verified']);self.assertEqual(destination.read_text(),text);self.assertEqual(destination.stat().st_mode & 0o777,0o600);self.assertEqual(source.read_bytes(),before)
            with self.assertRaises(FileExistsError):message_status.inspect_task(task,source,destination)
            bad=json.loads(source.read_text());bad['entries'][0]['text']='corrupted';source.write_text(json.dumps(bad))
            with self.assertRaisesRegex(RuntimeError,'digest mismatch'):message_status.inspect_task(task,source,Path(root)/'bad.txt')
            self.assertFalse((Path(root)/'bad.txt').exists())

if __name__=='__main__':unittest.main()
