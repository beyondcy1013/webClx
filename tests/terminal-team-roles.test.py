"""Pure skill role resolution checks; never contact or mutate live terminals."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('team', Path(__file__).resolve().parents[1] / '.codex/skills/webclx-terminal-collaboration/scripts/team.py')
team = importlib.util.module_from_spec(spec)
spec.loader.exec_module(team)


class TeamRoles(unittest.TestCase):
    def test_persisted_roles_override_names(self):
        sessions = [
            {'id': 'a', 'name': '旧管理者', 'group_role': '执行者'},
            {'id': 'b', 'name': '普通终端', 'group_role': '超级管理员'},
            {'id': 'c', 'name': '普通终端', 'group_role': '普通管理员'},
        ]
        roles = team.build_roles(sessions, ['a', 'b', 'c'], ['a', 'b', 'c'])
        self.assertEqual([r['role'] for r in roles], ['executor', 'super_admin', 'manager'])

    def test_explicit_super_admin_and_duplicate_resolution(self):
        sessions = [{'id': 'a', 'group_role': '超级管理员'}, {'id': 'b', 'group_role': '执行者'}]
        roles = team.build_roles(sessions, ['a', 'b'], ['a', 'b'], 'b')
        self.assertEqual([r['role'] for r in roles], ['manager', 'super_admin'])
        with self.assertRaises(ValueError):
            team.build_roles(sessions, ['a', 'b'], ['a', 'b'], 'missing')

    def test_name_inference_and_legacy_default(self):
        sessions = [{'id': 'a', 'name': '项目_超级管理员'}, {'id': 'b', 'name': '执行者'}]
        self.assertEqual(team.build_roles(sessions, ['a', 'b'], ['a', 'b'])[0]['role'], 'super_admin')
        self.assertEqual(team.build_roles([{'id': 'a'}, {'id': 'b'}], ['a', 'b'], ['a', 'b'])[0]['role'], 'manager')


if __name__ == '__main__':
    unittest.main()
