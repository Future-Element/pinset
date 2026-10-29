#!/usr/bin/env python3
"""Offline contracts for the one-release exception; never contact GitHub."""

import copy
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.error

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release_gate as gate


class ReleaseExceptionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='pinset-exception-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.git('init', '--quiet')
        files = {'Cargo.toml': '[workspace.package]\nversion = "3.1.1"\n',
                 'Cargo.lock': '# fixture lock\n',
                 'editors/vscode/package.json': '{"version":"3.1.1"}',
                 'website/package.json': '{"version":"3.1.1"}',
                 'release.json': '{"published":"3.1.1"}',
                 'CHANGELOG.md': '# Historical content\n',
                 '.github/workflows/release.yml': '\n'.join(before for before, _ in gate.EXCEPTION_WORKFLOW_REPLACEMENTS)
                     + '\npermissions: contents: read\nrun: cargo build --release --locked --target x86_64-pc-windows-msvc\n'
                     + 'run: gh attestation verify archive.zip\n'}
        for name, content in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(content, encoding='utf-8')
        self.commit('fixture implementation')
        self.base = self.git('rev-parse', 'HEAD').strip()
        # A real tiny Git history tests ancestry/diffs without relying on the
        # caller's checkout depth, credentials, remotes, or source permissions.
        replacement = patch.object(gate, 'EXCEPTION_IMPLEMENTATION_SHA', self.base)
        replacement.start()
        self.addCleanup(replacement.stop)
        (self.root / 'CHANGELOG.md').write_text('# One-release exception metadata\n')
        workflow = self.root / '.github/workflows/release.yml'
        value = workflow.read_text()
        for before, after in gate.EXCEPTION_WORKFLOW_REPLACEMENTS:
            value = value.replace(before, after)
        workflow.write_text(value)
        self.commit('fixture release metadata')
        self.calls = []
        endpoint = self.endpoint()
        self.documents = {
            endpoint: {'id': gate.EXCEPTION_CI_RUN_ID, 'head_sha': self.base,
                       'path': gate.EXCEPTION_CI_WORKFLOW, 'event': 'workflow_dispatch',
                       'run_attempt': 1, 'status': 'completed', 'conclusion': 'success',
                       'repository': {'full_name': gate.EXCEPTION_REPOSITORY},
                       'head_repository': {'full_name': gate.EXCEPTION_REPOSITORY}},
            endpoint + '/jobs?per_page=100': {'total_count': 4, 'jobs': [
                {'name': f'Native {target} (source)', 'head_sha': self.base,
                 'status': 'completed', 'conclusion': 'success'}
                for target in ('linux-x86_64', 'linux-aarch64', 'windows-x86_64', 'macos-aarch64')]},
            endpoint + '/artifacts?per_page=100': {'total_count': 5, 'artifacts': [
                {'name': name, **copy.deepcopy(value), 'expired': False,
                 'workflow_run': {'id': gate.EXCEPTION_CI_RUN_ID, 'head_sha': self.base}}
                for name, value in gate.EXCEPTION_ARTIFACTS.items()]},
        }
        self.report = self.make_report()

    def git(self, *arguments):
        environment = dict(os.environ, GIT_AUTHOR_NAME='Pinset fixture', GIT_AUTHOR_EMAIL='fixture@example.invalid',
                           GIT_COMMITTER_NAME='Pinset fixture', GIT_COMMITTER_EMAIL='fixture@example.invalid')
        return subprocess.check_output(['git', '-c', 'commit.gpgsign=false', '-c',
            'core.hooksPath=' + str(self.root / '.git/hooks'), *arguments], cwd=self.root,
            env=environment, text=True, stderr=subprocess.PIPE)

    def commit(self, message):
        self.git('add', '--all')
        self.git('commit', '--quiet', '-m', message)

    def endpoint(self):
        return f'repos/{gate.EXCEPTION_REPOSITORY}/actions/runs/{gate.EXCEPTION_CI_RUN_ID}'

    def query(self, endpoint):
        self.calls.append(endpoint)
        return copy.deepcopy(self.documents[endpoint])

    def make_report(self):
        return {'protocol': gate.EXCEPTION_PROTOCOL, 'tag': gate.EXCEPTION_TAG,
                'authorization': gate.EXCEPTION_AUTHORIZATION, 'implementation_commit': self.base,
                'commit': self.git('rev-parse', 'HEAD').strip(), 'source_clean': True,
                'source_fingerprint': gate.source_fingerprint(self.root),
                'lock_sha256': hashlib.sha256((self.root / 'Cargo.lock').read_bytes()).hexdigest(),
                'local_docker': copy.deepcopy(gate.EXCEPTION_DOCKER),
                'ci': {'repository': gate.EXCEPTION_REPOSITORY, 'workflow': gate.EXCEPTION_CI_WORKFLOW,
                       'run_id': gate.EXCEPTION_CI_RUN_ID, 'run_attempt': 1, 'head_sha': self.base,
                       'artifacts': copy.deepcopy(gate.EXCEPTION_ARTIFACTS)},
                'windows_candidate': copy.deepcopy(gate.EXCEPTION_WINDOWS_CANDIDATE),
                'unverified': list(gate.EXCEPTION_UNVERIFIED)}

    def validate(self, report=None, tag=None):
        return gate.validate(report or self.report, self.root, tag or gate.EXCEPTION_TAG, query=self.query)

    def test_scoped_release_with_real_git_ancestry_and_three_metadata_reads(self):
        self.assertEqual(self.validate(), self.report['commit'])
        self.assertEqual(self.calls, list(self.documents))

    def test_every_other_version_and_report_tag_are_rejected(self):
        for tag in ('v3.1.0', 'v3.1.2', 'v3.1.1-rc.1', 'v4.0.0'):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                self.validate(tag=tag)
        report = copy.deepcopy(self.report)
        report['tag'] = 'v3.1.2'
        with self.assertRaises(ValueError):
            self.validate(report)
        self.assertEqual(self.calls, [])

    def test_final_source_identity_and_tested_base_are_required(self):
        for key, value in (('implementation_commit', '0' * 40), ('commit', '0' * 40),
                           ('source_clean', False), ('source_fingerprint', '0' * 64),
                           ('lock_sha256', '0' * 64)):
            report = copy.deepcopy(self.report)
            report[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.validate(report)
        self.assertEqual(self.calls, [])

    def test_docker_skip_and_unverified_boundaries_cannot_be_hidden(self):
        for key, value in (('local_docker', {'status': 'passed'}), ('local_docker', {}),
                           ('unverified', []), ('authorization', 'assumed approval')):
            report = copy.deepcopy(self.report)
            report[key] = value
            with self.subTest(key=key, value=value), self.assertRaises(ValueError):
                self.validate(report)
        report = copy.deepcopy(self.report)
        report.pop('local_docker')
        with self.assertRaises(ValueError):
            self.validate(report)
        report = copy.deepcopy(self.report)
        report['suite'] = 'all'
        with self.assertRaises(ValueError):
            self.validate(report)

    def test_untested_business_build_and_dependency_changes_are_rejected(self):
        for name in ('crates/pinset/src/main.rs', '.cargo/config.toml', 'Cargo.lock', 'install.ps1'):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('changed untested implementation\n')
            self.commit('fixture untested change')
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'untested business/build/dependency'):
                self.validate(self.make_report())
            self.git('reset', '--hard', self.report['commit'])
        self.assertEqual(self.calls, [])

    def test_dirty_metadata_and_untracked_source_are_rejected(self):
        path = self.root / 'CHANGELOG.md'
        path.write_text('dirty allowed metadata\n')
        with self.assertRaisesRegex(ValueError, 'uncommitted changes'):
            self.validate(self.make_report())
        self.git('restore', '--', 'CHANGELOG.md')
        (self.root / 'untracked.rs').write_text('untested source\n')
        with self.assertRaisesRegex(ValueError, 'uncommitted changes'):
            self.validate(self.make_report())

    def test_workflow_build_attestation_and_permissions_cannot_change(self):
        path = self.root / '.github/workflows/release.yml'
        approved = path.read_text()
        for before, after in (('cargo build --release --locked --target x86_64-pc-windows-msvc', 'cargo build'),
                              ('run: gh attestation verify archive.zip\n', ''),
                              ('permissions: contents: read', 'permissions: write-all')):
            path.write_text(approved.replace(before, after))
            self.commit('fixture unauthorized workflow change')
            with self.subTest(before=before), self.assertRaisesRegex(ValueError, 'beyond the three approved'):
                self.validate(self.make_report())
        self.assertEqual(self.calls, [])

    def test_malformed_remote_metadata_is_rejected(self):
        for endpoint in self.documents:
            previous = self.documents[endpoint]
            self.documents[endpoint] = []
            with self.subTest(endpoint=endpoint), self.assertRaises(ValueError):
                self.validate()
            self.documents[endpoint] = previous

    def test_generated_output_is_not_a_source_change(self):
        path = self.root / 'output/report.json'
        path.parent.mkdir()
        path.write_text('{"real_evidence":true}\n')
        self.assertEqual(self.validate(), self.report['commit'])

    def test_nonancestor_cannot_supply_native_ci_identity(self):
        empty = self.root / '.git/empty-tree-fixture'
        empty.write_bytes(b'')
        orphan = self.git('hash-object', '-w', '-t', 'tree', str(empty)).strip()
        # An unrelated but valid commit makes merge-base fail deterministically.
        unrelated = self.git('commit-tree', orphan, '-m', 'unrelated fixture').strip()
        with patch.object(gate, 'EXCEPTION_IMPLEMENTATION_SHA', unrelated):
            report = self.make_report()
            report['implementation_commit'] = unrelated
            report['ci']['head_sha'] = unrelated
            with self.assertRaisesRegex(ValueError, 'not an ancestor'):
                self.validate(report)

    def test_ci_report_identity_and_windows_candidate_digests_are_fixed(self):
        for part, key, value in (('ci', 'run_id', 1), ('ci', 'head_sha', '0' * 40),
                                 ('ci', 'repository', 'attacker/pinset'),
                                 ('windows_candidate', 'archive_sha256', '0' * 64),
                                 ('windows_candidate', 'import_inspection', 'skipped')):
            report = copy.deepcopy(self.report)
            report[part][key] = value
            with self.subTest(part=part, key=key), self.assertRaises(ValueError):
                self.validate(report)
        report = copy.deepcopy(self.report)
        report['windows_candidate']['binaries']['pinset-shim.exe'] = '0' * 64
        with self.assertRaises(ValueError):
            self.validate(report)

    def test_remote_run_identity_success_and_attempt_are_required(self):
        endpoint = self.endpoint()
        original = copy.deepcopy(self.documents[endpoint])
        for key, value in (('id', 1), ('head_sha', '0' * 40), ('path', '.github/workflows/other.yml'),
                           ('status', 'in_progress'), ('conclusion', 'failure'), ('event', 'push'),
                           ('run_attempt', 2), ('repository', {'full_name': 'other/project'}),
                           ('head_repository', {'full_name': 'other/project'})):
            self.documents[endpoint] = {**original, key: value}
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'CI run identity/status'):
                self.validate()

    def test_all_four_native_jobs_must_pass_on_the_tested_sha(self):
        endpoint = self.endpoint() + '/jobs?per_page=100'
        original = copy.deepcopy(self.documents[endpoint])
        for key, value in (('head_sha', '0' * 40), ('status', 'in_progress'), ('conclusion', 'cancelled')):
            self.documents[endpoint] = copy.deepcopy(original)
            self.documents[endpoint]['jobs'][2][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'four pinned native CI jobs'):
                self.validate()
        self.documents[endpoint] = copy.deepcopy(original)
        self.documents[endpoint]['jobs'].pop()
        with self.assertRaises(ValueError):
            self.validate()
        self.documents[endpoint] = copy.deepcopy(original)
        self.documents[endpoint]['jobs'][0] = self.documents[endpoint]['jobs'][1]
        with self.assertRaises(ValueError):
            self.validate()

    def test_artifacts_must_match_reviewed_ids_hashes_and_source(self):
        endpoint = self.endpoint() + '/artifacts?per_page=100'
        original = copy.deepcopy(self.documents[endpoint])
        for key, value in (('id', 1), ('digest', 'sha256:' + '0' * 64), ('size_in_bytes', 0),
                           ('expired', True), ('workflow_run', {'id': 1, 'head_sha': self.base})):
            self.documents[endpoint] = copy.deepcopy(original)
            self.documents[endpoint]['artifacts'][-1][key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, 'artifact identity/digest'):
                self.validate()
        self.documents[endpoint] = copy.deepcopy(original)
        self.documents[endpoint]['artifacts'].pop()
        with self.assertRaises(ValueError):
            self.validate()

    def test_unverifiable_remote_metadata_fails_without_a_fallback(self):
        def unavailable(_endpoint):
            raise ValueError('public metadata unavailable')
        with self.assertRaisesRegex(ValueError, 'metadata unavailable'):
            gate.validate(self.report, self.root, gate.EXCEPTION_TAG, query=unavailable)
        for status in (403, 429):
            failure = urllib.error.HTTPError('https://api.github.com/', status, 'blocked', {}, None)
            with patch.object(gate.urllib.request, 'urlopen', side_effect=failure) as request:
                with self.subTest(status=status), self.assertRaisesRegex(ValueError, 'could not be verified'):
                    gate.public_github_metadata(self.endpoint())
                self.assertEqual(request.call_count, 1)
                self.assertIsNone(request.call_args.args[0].get_header('Authorization'))
                self.assertEqual(request.call_args.kwargs['timeout'], 30)

    def test_ordinary_gate_still_rejects_native_reports_and_accepts_valid_local_metadata(self):
        report = {**self.report, 'protocol': 'pinset-verification/3', 'suite': 'all', 'execution': 'native-macos'}
        with self.assertRaisesRegex(ValueError, 'not produced by local Docker'):
            gate.validate(report, self.root, gate.EXCEPTION_TAG)
        report = {key: self.report[key] for key in ('commit', 'source_clean', 'source_fingerprint', 'lock_sha256')}
        report.update(protocol='pinset-verification/3', suite='all', execution='local-docker',
            suites={name: {'status': 'passed'} for name in ('fast', 'acceptance', 'platform', 'integrations')},
            commands=[{'exit_code': 0}], image_digests={'verify': 'sha256:' + '1' * 64},
            evidence_files={name: {'sha256': '2' * 64, 'bytes': 1} for name in
                ('contracts.json', 'distribution.json', 'cargo-security.json', 'real-sdks.json', 'platform.json',
                 'integrations.json', 'editor-host.json', 'flutter-contracts.json', 'frozen-selectors.json')},
            arm64_execution='QEMU emulation', unverified=['Windows native runtime', 'macOS native runtime', gate.LARGE_ARTIFACT_BOUNDARY],
            runtime_exemptions=gate.RUNTIME_EXEMPTIONS)
        self.assertEqual(gate.validate(report, self.root, gate.EXCEPTION_TAG), self.report['commit'])
        self.assertEqual(self.calls, [])


def verify_release_exception():
    stream = io.StringIO()
    result = unittest.TextTestRunner(stream=stream).run(
        unittest.defaultTestLoader.loadTestsFromTestCase(ReleaseExceptionTests))
    if not result.wasSuccessful():
        raise AssertionError('One-release exception contracts failed:\n' + stream.getvalue())
    return {'passed': True, 'tests': result.testsRun,
            'checks': ['v3.1.1 only; exact source/lock/clean commit; real Git ancestry and metadata-only diff',
                       'release workflow permits only the three approved description changes',
                       'Docker remains skipped; no false all-suite pass',
                       'fixed successful CI run, four native jobs and five artifact identities/digests',
                       'fixed inspected Windows ZIP and paired executable hashes',
                       'public metadata failure closes the gate; ordinary Docker gate unchanged'],
            'scope': 'offline Git and API fixtures; no GitHub requests or project commits'}


if __name__ == '__main__':
    print(json.dumps(verify_release_exception(), indent=2))
