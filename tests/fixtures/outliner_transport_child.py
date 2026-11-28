"""Synthetic JSON-lines child for transport failures; no application imports."""
import json
import os
from pathlib import Path
import sys
import time

mode, log = sys.argv[1:3]
if sys.argv[3:] == ['ready']:
    print('READY', flush=True)
for line in sys.stdin:
    request = json.loads(line)
    with open(log, 'a', encoding='utf-8') as stream:
        stream.write(json.dumps(request) + '\n')
    if mode == 'timeout':
        time.sleep(60)
    if request['op'] == 'save':
        Path(request['args']['path']).write_text(request['args']['markdown'], encoding='utf-8')
    if mode in ('exit', 'write_exit'):
        sys.exit(1)
    if mode == 'broken':
        print('{broken', flush=True)
        continue
    value = {'id': request['id'], 'ok': True,
             'result': {'document': request.get('document'), 'pid': os.getpid()}}
    if mode == 'mismatch':
        value['id'] = 'wrong-request'
    if mode == 'envelope':
        value.pop('result')
    if mode == 'error':
        value = {'id': request['id'], 'ok': False,
                 'error': {'code': 'QUERY_TOO_SHORT', 'message': '짧은 검색어'}}
    print(json.dumps(value), flush=True)
    if mode == 'double':
        print(json.dumps(value), flush=True)
