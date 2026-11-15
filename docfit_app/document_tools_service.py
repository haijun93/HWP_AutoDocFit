"""UI-independent text input and conversion tools."""
import json
from pathlib import Path
from docfit_core import pasted_text, labeled_text, writing_aids
from .api_contract import ServiceError

PIPELINE_ORDER = ('clean', 'convert', 'labels', 'mask_rrn', 'amount', 'weekdays', 'commas')


class DocumentToolsService:
    def __init__(self, folder):
        self.folder = Path(folder)

    def _options_file(self):
        return self.folder / 'document_tools_options.json'

    def _load_options(self):
        path = self._options_file()
        if path.is_file():
            try:
                with path.open('r', encoding='utf-8') as stream:
                    data = json.load(stream)
                    if isinstance(data, list):
                        return [x for x in data if isinstance(x, str)]
                    if isinstance(data, dict) and isinstance(data.get('options'), list):
                        return [x for x in data['options'] if isinstance(x, str)]
            except Exception:
                pass
        return ['convert']

    def _save_options(self, options):
        if not isinstance(options, (list, tuple)):
            raise ServiceError('INVALID_INPUT', '옵션 목록을 확인해 주세요.')
        path = self._options_file()
        try:
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open('w', encoding='utf-8') as stream:
                json.dump({'options': list(options)}, stream, ensure_ascii=False)
        except OSError:
            pass

    @staticmethod
    def _text(payload):
        value = payload.get('text', '')
        if not isinstance(value, str) or len(value.encode('utf-8')) > 1_000_000:
            raise ServiceError('INVALID_TEXT', '입력은 UTF-8 1MB 이하의 텍스트여야 합니다.')
        return value

    def command(self, name, payload):
        if not isinstance(payload, dict):
            raise ServiceError('INVALID_INPUT', '입력을 확인해 주세요.')

        if name == 'get_options':
            return {'options': self._load_options()}

        if name == 'save_options':
            opts = payload.get('options', [])
            self._save_options(opts)
            return {'ok': True, 'options': list(opts)}

        text = self._text(payload)
        if name == 'add_text':
            if not text.strip():
                raise ServiceError('EMPTY_TEXT', '추가할 텍스트를 입력해 주세요.')
            import uuid
            folder = self.folder / '입력문서'
            try:
                folder.mkdir(parents=True, exist_ok=True)
                path = folder / ('붙여넣기-' + uuid.uuid4().hex + '.txt')
                with path.open('x', encoding='utf-8') as stream:
                    stream.write(text)
            except OSError as exc:
                raise ServiceError('TEXT_SAVE_FAILED', '입력 문서를 저장하지 못했습니다.') from exc
            return {'path': str(path)}

        transforms = {
            'clean': pasted_text.clean_pasted_text,
            'convert': pasted_text.outline_pasted_text,
            'labels': labeled_text.label_outline_text,
            'amount': writing_aids.hangulize_amounts,
            'commas': writing_aids.add_commas,
            'weekdays': writing_aids.add_weekdays,
            'age': writing_aids.ages_for_lines,
            'mask_rrn': writing_aids.mask_rrn,
        }

        try:
            if name == 'pipeline':
                options = payload.get('options')
                if options is None:
                    options = self._load_options()
                elif not isinstance(options, (list, tuple)):
                    raise ServiceError('INVALID_INPUT', '옵션 목록을 확인해 주세요.')
                else:
                    self._save_options(options)

                result = text
                for opt in PIPELINE_ORDER:
                    if opt in options and opt in transforms:
                        result = transforms[opt](result)
                return {'text': result}

            if name in transforms:
                return {'text': transforms[name](text)}
            if name == 'date_diff':
                dates = [writing_aids.parse_date(line) for line in text.splitlines() if line.strip()]
                if len(dates) != 2 or any(value is None for value in dates):
                    raise ValueError('dates')
                return {'text': str(writing_aids.days_between(*dates)) + '일'}
            if name == 'table_calc':
                return {
                    'text': writing_aids.table_to_text(
                        writing_aids.calc_table(
                            writing_aids.parse_table(text),
                            payload.get('operation', 'col_sum'),
                        )
                    )
                }
        except ServiceError:
            raise
        except (ValueError, TypeError, KeyError, OSError) as exc:
            raise ServiceError('TOOL_INPUT', '입력 형식·날짜·계산 종류·문서 경로를 확인해 주세요.') from exc
        raise ServiceError('INVALID_TOOL', '지원하지 않는 텍스트 변환 명령입니다.')
