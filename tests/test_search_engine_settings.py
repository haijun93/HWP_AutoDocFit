import tempfile
import unittest
from pathlib import Path

from docfit_app.configuration_service import ConfigurationService
from docfit_app.settings_service import SettingsService
from docfit_app.settings_schema import DEFAULTS, normalize_search_engine, validate_search_engine
from docfit_app.api_contract import ServiceError
from docfit_app.timemachine_service import TimemachineService


class SearchEngineSettingsTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.store = SettingsService(self.root / 'settings.json')
        self.config = ConfigurationService(self.store)

    def test_defaults_and_choice_schema(self):
        self.assertEqual(DEFAULTS['search_engine'], 'kfind')
        snapshot = self.config.snapshot()
        field = next(f for f in snapshot['fields'] if f['key'] == 'search_engine')
        self.assertEqual(field['choices'], ['kfind', 'legacy'])
        self.assertEqual(snapshot['values']['search_engine'], 'kfind')

    def test_save_both_values_and_restart(self):
        for value in ('legacy', 'kfind'):
            self.config.save({'search_engine': value}, self.config.snapshot()['revision'])
            self.assertEqual(ConfigurationService(SettingsService(self.root / 'settings.json')).snapshot()['values']['search_engine'], value)

    def test_invalid_choice_cannot_write(self):
        for bad in ('shadow', '', None, [], 'KFIND'):
            with self.subTest(bad=bad), self.assertRaises(ServiceError):
                self.config.save({'search_engine': bad}, self.config.snapshot()['revision'])
        self.assertFalse((self.root / 'settings.json').exists())

    def test_invalid_saved_value_normalized(self):
        self.store.update(lambda data: data.update(search_engine=['bad']))
        self.assertEqual(self.config.snapshot()['values']['search_engine'], 'kfind')
        self.assertEqual(normalize_search_engine(None), 'kfind')
        with self.assertRaises(ValueError):
            validate_search_engine('shadow')

    def test_tm_reads_shared_setting_without_restart(self):
        service = TimemachineService(self.root, db_path=self.root / 'timeline.db',
            thumb_dir=self.root / 'thumbs', search_engine_provider=lambda: self.store.read().get('search_engine'))
        self.addCleanup(service.close)
        self.assertEqual(service._search_engine(), 'kfind')
        self.config.save({'search_engine':'legacy'}, self.config.snapshot()['revision'])
        state = service.content_status()
        self.assertEqual(state['search_engine_status']['transition'], 'legacy')
        self.assertEqual(state['engine_reason'], 'configured_legacy')
        self.assertEqual(service.command('settings_get', {})['settings']['search_engine'], 'legacy')
        self.config.save({'search_engine':'kfind'}, self.config.snapshot()['revision'])
        self.assertEqual(service._search_engine(), 'kfind')

    def test_status_transitions_preserve_compatibility(self):
        service = TimemachineService(self.root, db_path=self.root / 'timeline.db', thumb_dir=self.root / 'thumbs')
        self.addCleanup(service.close)
        for corpus, engine, expected in [('pending','legacy','pending'),('seeding','legacy','seeding'),('synced','kfind','ready'),('failed','legacy','fallback')]:
            service._engine_state.update(corpus=corpus, engine=engine)
            status = service._engine_status()
            self.assertEqual(status['engine'], engine)
            self.assertEqual(status['search_engine_status']['transition'], expected)
            self.assertEqual(status['search_engine_status']['connection'], service.kfind.state())
