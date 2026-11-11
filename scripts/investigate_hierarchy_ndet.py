import json,sys,subprocess,types,collections
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT))
from docfit_app import hierarchy_bridge as b
from docfit_core.style_hierarchy import analyze_hierarchy
p=[dict(text=t,indent=i,font_size=s) for t,i,s in [('1. 추진 배경',0,15),('가. 세부 계획',10,13),('ㅇ 사업의 추진 현황을 다음과 같이 보고합니다.',20,13),('- 세부 내용 및 향후 일정',30,13)]]
f=ROOT/'tests/fixtures/T1111_NDET.json';f.write_text(json.dumps(p,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
old=types.ModuleType('old');old.__file__=b.__file__;exec(compile(subprocess.check_output(['git','show','e60fed03^:docfit_app/hierarchy_bridge.py']),b.__file__,'exec'),old.__dict__)
out={}
for name,fn in [('dll',lambda:b.run_library(b.find_library(),p)),('exe',lambda:b.run_executable(b.find_executable(),p)),('python',lambda:analyze_hierarchy(p)),('before_dll',lambda:old.run_library(b.find_library(),p))]:
 c=collections.Counter(json.dumps(fn(),ensure_ascii=False,sort_keys=True) for _ in range(1000));out[name]={'calls':1000,'variants':[{'count':n,'result':json.loads(r)} for r,n in c.items()]};print(name,len(c),flush=True)
(ROOT/'docs/T1111_NDET_MEASUREMENTS.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
