"""Offline tests: no GitHub calls, sudo, service changes, or production backups."""
import importlib.util
from datetime import datetime, timedelta, timezone
from pathlib import Path
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('github_backups', Path(__file__).resolve().parents[1] / 'src/server/github_backups.py')
b = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(b)
UTC = timezone.utc


class RetentionTests(unittest.TestCase):
    def test_density_and_boundaries(self):
        now = datetime(2026, 9, 7, 12, tzinfo=UTC)
        cases = [(timedelta(hours=23), 'recent'), (timedelta(days=1, seconds=1), 'daily'),
                 (timedelta(days=7, seconds=1), 'weekly'), (timedelta(days=32), 'monthly'),
                 (timedelta(days=366), 'yearly'), (timedelta(days=365*6), 'five-year')]
        for age, tier in cases:
            self.assertEqual(b.retention_key(now-age, now)[0], tier)
        daily = [(datetime(2026, 9, 5, h, tzinfo=UTC), Path(str(h))) for h in (0, 6, 12, 18)]
        self.assertEqual(b.retained_archives(daily, now), {Path('6'), Path('18')})
        self.assertEqual(len(b.retained_archives([(now-timedelta(hours=h), Path(str(h))) for h in (0,6,12,18)], now)), 4)

    def test_week_month_year_and_five_year_counts(self):
        now = datetime(2026, 9, 30, tzinfo=UTC)
        for start, duration, expected in [(datetime(2026,9,7,tzinfo=UTC), 7, 4),
                                          (datetime(2026,7,1,tzinfo=UTC), 31, 4),
                                          (datetime(2024,1,1,tzinfo=UTC), 366, 4),
                                          (datetime(2010,1,1,tzinfo=UTC), 1826, 2)]:
            archives = [(start+timedelta(hours=6*i), Path(str(i))) for i in range(duration*4)]
            self.assertEqual(len(b.retained_archives(archives, now)), expected)

    def test_newest_survives_even_when_ancient(self):
        date = datetime(1990,1,1,tzinfo=UTC)
        self.assertEqual(b.retained_archives([(date,Path('last'))], datetime.now(UTC)), {Path('last')})

    def test_pruning_only_managed_names_in_active_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            old = root/'old'; old.mkdir()
            current = root/'new'; current.mkdir()
            now = datetime(2026,9,7,12,tzinfo=UTC)
            names = []
            for h in (0,6,12,18):
                date = datetime(2026,9,5,h,tzinfo=UTC)
                name = 'backup_' + (date.strftime(b.ARCHIVE_TIME_FORMAT) if h else date.strftime('%Y%m%dT%H%M%S%fZ')) + '.tar.zst'
                names.append(name)
                (current/name).touch(); (old/name).touch()
                (current/(name+'.sha256')).touch()
            (current/'personal.tar.zst').touch()
            b.prune(current,now)
            self.assertEqual(len(list(old.iterdir())),4)
            self.assertTrue((current/'personal.tar.zst').exists())
            self.assertFalse((current/names[0]).exists())
            self.assertFalse((current/(names[0]+'.sha256')).exists())


class WorkflowTests(unittest.TestCase):
    def test_disabled_issues_do_not_fail_fork_backups(self):
        with tempfile.TemporaryDirectory() as temporary:
            api = b.GitHub()
            with patch.object(api, 'pages', return_value=[]) as pages:
                b.update_metadata(api, 'owner', {'name': 'fork', 'has_issues': False}, Path(temporary))
                pages.assert_called_once_with('/repos/owner/fork/releases')
            self.assertEqual(json.loads((Path(temporary)/'metadata/issues.json').read_text()), [])

    def test_api_rate_limit_waits_and_retries_without_credentials(self):
        import io
        response = unittest.mock.MagicMock()
        response.__enter__.return_value = response
        response.read.return_value = b'[]'
        response.headers = {}
        error = b.urllib.error.HTTPError('https://api.github.com/test', 403, 'limit',
                                       {'X-RateLimit-Remaining': '0', 'Retry-After': '1'}, io.BytesIO())
        with patch.object(b.urllib.request, 'urlopen', side_effect=[error, response]) as request, patch.object(b.time, 'sleep') as sleep:
            result, _ = b.GitHub().request('https://api.github.com/test')
            self.assertEqual(result, [])
            self.assertEqual(request.call_count, 2)
            sleep.assert_called_once_with(1)
            self.assertNotIn('Authorization', request.call_args.args[0].headers)

    def test_pagination(self):
        api = b.GitHub()
        with patch.object(api,'request',side_effect=[([{'id':1}],{'Link':'<https://api.github.com/test?page=2>; rel="next"'}),([{'id':2}],{})]) as request:
            self.assertEqual(api.pages('/test'),[{'id':1},{'id':2}])
            self.assertEqual(request.call_count,2)

    def test_metadata_failure_preserves_previous(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); (root/'metadata').mkdir(); (root/'metadata/old').touch()
            api = b.GitHub()
            with patch.object(api,'pages',side_effect=RuntimeError('offline')):
                with self.assertRaises(RuntimeError):
                    b.update_metadata(api,'owner',{'name':'repo'},root)
            self.assertTrue((root/'metadata/old').exists())

    def test_assets_cached_and_metadata_contains_closed_issues_and_comments(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            api=b.GitHub()
            asset={'id':2,'name':'download.bin','updated_at':'today','size':3,'browser_download_url':'https://example.invalid/asset'}
            def pages(endpoint):
                if endpoint.endswith('/assets'): return [asset]
                if endpoint.endswith('/releases'): return [{'id':1}]
                return [{'body':'data'}]
            def download(url,download=None): download.write_bytes(b'abc')
            with patch.object(api,'pages',side_effect=pages) as queries, patch.object(api,'request',side_effect=download) as downloads:
                b.update_metadata(api,'owner',{'name':'repo'},root)
                b.update_metadata(api,'owner',{'name':'repo'},root)
                self.assertEqual(downloads.call_count,1)
                self.assertTrue(any('state=all' in call.args[0] for call in queries.call_args_list))
            self.assertTrue((root/'metadata/issue-comments.json').exists())

    def test_failed_repo_does_not_prune_and_other_repos_continue(self):
        repos=[{'id':i,'name':f'repo{i}','private':False,'owner':{'login':'HungLo2020'}} for i in (1,2)]
        with tempfile.TemporaryDirectory() as temporary, patch.object(b,'WORK_DIRECTORY',Path(temporary)/'work'), patch.object(b,'BACKUP_DESTINATION',Path(temporary)/'out'), patch.object(b.GitHub,'pages',return_value=repos), patch.object(b,'update_mirror',side_effect=[RuntimeError('failed'),None]), patch.object(b,'update_metadata'), patch.object(b,'create_archive') as archive, patch.object(b,'prune') as prune:
            with self.assertRaisesRegex(RuntimeError,'repo1'):
                b.backup()
            self.assertEqual(archive.call_count,1)
            self.assertEqual(prune.call_count,1)
            self.assertEqual(prune.call_args.args[0].name,'repo2')

    def test_failed_archive_never_prunes(self):
        repo={'id':1,'name':'repo','private':False,'owner':{'login':'HungLo2020'}}
        with tempfile.TemporaryDirectory() as temporary, patch.object(b,'WORK_DIRECTORY',Path(temporary)/'work'), patch.object(b,'BACKUP_DESTINATION',Path(temporary)/'out'), patch.object(b.GitHub,'pages',return_value=[repo]), patch.object(b,'update_mirror'), patch.object(b,'update_metadata'), patch.object(b,'create_archive',side_effect=RuntimeError('disk full')), patch.object(b,'prune') as prune:
            with self.assertRaises(RuntimeError): b.backup()
            prune.assert_not_called()

    def test_lock_excludes_second_run(self):
        with tempfile.TemporaryDirectory() as temporary:
            with b.job_lock(Path(temporary)):
                with self.assertRaisesRegex(RuntimeError,'Another'):
                    with b.job_lock(Path(temporary)): pass

    def test_clone_or_fetch_automatically(self):
        with tempfile.TemporaryDirectory() as temporary:
            mirror=Path(temporary)/'repo.git'
            def command(args,**kwargs):
                if 'clone' in args: Path(args[-1]).mkdir()
                return subprocess.CompletedProcess(args,0,stdout='ref: refs/heads/main\tHEAD\n')
            with patch.object(b,'run',side_effect=command) as commands:
                b.update_mirror('https://github.com/example/repo.git',mirror)
                self.assertTrue(any('clone' in call.args[0] for call in commands.call_args_list))
                commands.reset_mock()
                b.update_mirror('https://github.com/example/repo.git',mirror)
                self.assertFalse(any('clone' in call.args[0] for call in commands.call_args_list))
                self.assertTrue(any('fetch' in call.args[0] for call in commands.call_args_list))

    def test_install_first_and_repeat_only_manages_fixed_units(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for exists in (False,True):
                def command(args,**kwargs):
                    return subprocess.CompletedProcess(args,0,stdout='loaded' if exists else 'not-found')
                with patch.object(b.Path,'home',return_value=root), patch.object(b,'WORK_DIRECTORY',root/'work'), patch.object(b,'BACKUP_DESTINATION',root/'downloads'), patch.object(b.os,'geteuid',return_value=1000), patch.object(b,'run',side_effect=command) as commands:
                    b.install()
                    self.assertTrue((root/'.local/lib/github-backups/GitHubBackups.py').exists())
                    stops=[c for c in commands.call_args_list if 'stop' in c.args[0]]
                    self.assertEqual(len(stops),2 if exists else 0)
                    self.assertFalse((root/'downloads').exists())
                    self.assertFalse(any('--run' in c.args[0] for c in commands.call_args_list))

    @unittest.skipUnless(shutil.which('zstd') and shutil.which('git'),'git/zstd required')
    def test_real_archive_roundtrip_restores_git_history(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); repo=root/'source'; repo.mkdir()
            subprocess.run(['git','init',str(repo)],check=True,capture_output=True)
            (repo/'hello.txt').write_text('hello')
            subprocess.run(['git','-C',str(repo),'add','.'],check=True)
            subprocess.run(['git','-C',str(repo),'-c','user.name=Test','-c','user.email=test@example.invalid','commit','-m','fixture'],check=True,capture_output=True)
            data=root/'data'; data.mkdir()
            subprocess.run(['git','clone','--mirror',str(repo),str(data/'repository.git')],check=True,capture_output=True)
            (data/'metadata').mkdir(); (data/'metadata/issues.json').write_text('[]')
            (data/'release-assets').mkdir(); (data/'release-assets/asset.bin').write_bytes(b'abc')
            now = datetime(2026, 9, 8, 12, 34, 56, tzinfo=UTC)
            archive=b.create_archive(data,root/'out',now)
            self.assertEqual(archive.name, 'backup_2026-09-08_12-34-56_UTC.tar.zst')
            with self.assertRaisesRegex(RuntimeError, 'already exists'):
                b.create_archive(data,root/'out',now)
            restored=root/'restored';restored.mkdir()
            subprocess.run(['tar','--zstd','-xf',str(archive),'-C',str(restored)],check=True)
            result=subprocess.run(['git','--git-dir',str(restored/'repository.git'),'show','HEAD:hello.txt'],check=True,capture_output=True,text=True)
            self.assertEqual(result.stdout,'hello')
            self.assertEqual((restored/'release-assets/asset.bin').read_bytes(),b'abc')
            self.assertEqual(archive.with_name(archive.name+'.sha256').read_text().split()[0],b.file_digest(archive))


if __name__ == '__main__': unittest.main()
