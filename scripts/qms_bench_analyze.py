#!/usr/bin/env python3
"""Analyze qms_bench schema v1 results using only the Python standard library."""
from __future__ import annotations
import argparse, json, math, statistics, sys
from collections import defaultdict
from pathlib import Path

SUPPORTED_SCHEMA = {1}
REGRESSION_RATIO = 1.20
MIN_SAMPLES = 3
MIN_RUNS = 3
ENGINE_ALIASES = {"kfind_names": "몽땅검색", "kfind": "몽땅검색", "everything": "Everything", "everything_http": "Everything", "everything_es": "Everything"}

class InputError(ValueError): pass

def load_result(path):
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise InputError(f"입력 파일을 읽을 수 없습니다 ({path}): {exc}") from exc
    if not isinstance(data, dict): raise InputError(f"결과 JSON은 객체여야 합니다: {path}")
    version = data.get("schema_version")
    if version not in SUPPORTED_SCHEMA: raise InputError(f"지원하지 않는 schema_version입니다 ({path}): {version!r}")
    for field, typ in (("run", dict), ("queries", list), ("resources", dict), ("summary", dict)):
        if field not in data: raise InputError(f"필수 필드가 없습니다 ({path}): {field}")
        if not isinstance(data[field], typ): raise InputError(f"필드 형식이 올바르지 않습니다 ({path}): {field}")
    warnings=[]
    for field in ("started_at", "machine", "indexes"):
        if field not in data["run"]: warnings.append(f"{path}: run.{field} 메타데이터 없음")
    for i,q in enumerate(data["queries"]):
        if not isinstance(q,dict): raise InputError(f"질의 레코드는 객체여야 합니다 ({path}, {i})")
        # Current qms_bench writer stores digest/statistics/result_count/timeouts/errors.
        if "digest" not in q and not all(k in q for k in ("id","kind","query_hash")):
            warnings.append(f"{path}: queries[{i}] 식별 필드 일부 없음")
    return data,warnings

def pct(vals,p):
    a=sorted(float(x) for x in vals if isinstance(x,(int,float)) and math.isfinite(x))
    if not a:return None
    pos=(len(a)-1)*p; lo=int(pos); hi=math.ceil(pos)
    return a[lo]+(a[hi]-a[lo])*(pos-lo)

def stats(q, engine):
    e=q.get("engines",{}).get(engine,{}) if isinstance(q.get("engines"),dict) else {}
    raw=e.get("latency_ms")
    if isinstance(raw,list): vals=raw
    else:
        st=e.get("summary") or q.get("statistics") or {}
        # Native writer's per-engine-free statistics are usable for its single-engine run.
        vals=[]
        if st.get("p50") is not None: vals=[st["p50"]]
        return {"p50":st.get("p50"),"p95":st.get("p95"),"n":st.get("count",len(vals)) or 0}
    return {"p50":pct(vals,.5),"p95":pct(vals,.95),"n":len(vals)}

def ident(q):
    d=q.get("digest",{})
    return {"id":d.get("id",q.get("id","?")),"kind":d.get("kind",q.get("kind","unknown")),"length":d.get("query_length",q.get("query_length","?")),"hash":d.get("query_hash",q.get("query_hash","")),"text":d.get("query_text",q.get("query_text"))}

def result_band(n):
    if n is None:return "미상"
    if n==0:return "0건"
    if n<=10:return "1–10건"
    if n<=100:return "11–100건"
    if n<=1000:return "101–1,000건"
    return "1,001건 이상"

def query_metrics(q):
    i=ident(q); engines=q.get("engines",{})
    k=stats(q,"kfind_names") if engines else {"p50":(q.get("statistics") or {}).get("p50"),"p95":(q.get("statistics") or {}).get("p95"),"n":(q.get("statistics") or {}).get("count",0)}
    e=stats(q,"everything")
    count=q.get("result_count")
    if engines: count=engines.get("kfind_names",{}).get("result_count",count)
    return i,k,e,count

def qlabel(i):
    # Never synthesize source text; only use text if it was explicitly stored.
    return str(i["text"]) if i["text"] is not None else f"{i['kind']} / {i['length']}글 / {i['hash'][:16]}"

def analyze(data_list):
    groups=defaultdict(list); discrepancies=[]; slow=[]; failures=[]; all_metrics=[]
    for run_idx,data in enumerate(data_list):
        for q in data["queries"]:
            i,k,e,count=query_metrics(q); item=(run_idx,q,i,k,e,count); all_metrics.append(item)
            band=result_band(count)
            groups[(i["kind"],str(i["length"]),band)].append(item)
            if e["n"] and k["n"] and (e["n"]<3 or k["n"]<3): pass
            if k["p50"] is not None and e["p50"] not in (None,0): slow.append((k["p50"]/e["p50"],item))
            eng=q.get("engines",{})
            if eng:
                ke=eng.get("kfind_names",{}); ee=eng.get("everything",{})
                if ke.get("result_count") != ee.get("result_count") or q.get("top_n_path_set_match") is not None and q.get("top_n_path_set_match") != 1:
                    discrepancies.append((item,ke.get("result_count"),ee.get("result_count"),q.get("top_n_path_set_match")))
            errors=q.get("errors",0); timeouts=q.get("timeouts",0)
            if isinstance(errors,list): errors=len(errors)
            if isinstance(timeouts,dict): timeouts=sum(int(x or 0) for x in timeouts.values())
            if errors or timeouts or any(q.get("engines",{}).get(n,{}).get(k) for n in q.get("engines",{}) for k in ("errors","timeouts")):
                failures.append((item,errors,timeouts))
    return groups,discrepancies,sorted(slow,key=lambda x:x[0],reverse=True),failures,all_metrics

def ratio_fmt(a,b): return "—" if a is None or b in (None,0) else f"{a/b:.2f}x"
def resource_rows(data):
    r=data.get("resources",{})
    if isinstance(r,list): return r
    if isinstance(r,dict):
        if isinstance(r.get("targets"),list): return r["targets"]
        return [r]
    return []
def resource_info(data):
    out=[]
    for r in resource_rows(data):
        out.append({k:r.get(k) for k in ("engine","role","pid","status","unavailable_reason","working_set_peak_bytes","working_set_avg_bytes","private_peak_bytes","private_delta_bytes","cpu_delta_ms","read_bytes_delta","write_bytes_delta","read_ops_delta","write_ops_delta","idle_baseline") if k in r})
    return out

def regression(data_list):
    by=defaultdict(list)
    for d in data_list:
        run=d["run"]; key=(run.get("query_set_hash",run.get("query_set_version")),run.get("adapter"),run.get("top_n"))
        by[key].append(d)
    out=[]
    for key,runs in by.items():
        if len(runs)<2: continue
        base,latest=runs[0],runs[-1]
        bm={ident(q)["hash"]:query_metrics(q)[1] for q in base["queries"]}; lm={ident(q)["hash"]:query_metrics(q)[1] for q in latest["queries"]}
        common=[h for h in bm.keys()&lm.keys() if bm[h]["p50"] is not None and lm[h]["p50"] is not None and bm[h]["p50"]>0]
        for h in common:
            b,l=bm[h],lm[h]; change=l["p50"]/b["p50"]-1
            out.append((h,change,"판정 보류" if min(b["n"],l["n"])<MIN_SAMPLES or len(runs)<MIN_RUNS else "회귀 후보" if change>=REGRESSION_RATIO-1 else "회귀 아님",len(runs),b,l))
    return out

def render_report(data_list, warnings=(), sources=()):
    groups,discrepancies,slow,failures,metrics=analyze(data_list)
    lines=["# 벤치마크 분석 보고서","",f"입력 실행: {len(data_list)}개", "", "## 실행·비교 유효성", ""]
    for idx,d in enumerate(data_list,1):
        run=d["run"]; lines.append(f"- 실행 {idx}: ID `{run.get('run_id','unknown')}`, 어댑터 `{run.get('adapter','unknown')}`, Everything `{(run.get('everything') or {}).get('status','unknown')}`, 질의 세트 `{run.get('query_set_hash',run.get('query_set_version','unknown'))}`, 반복 `{run.get('repeats','?')}")
    lines += ["", "동일 질의 세트 해시 필드가 없는 현재 크레이트 출력은 `query_set_version`으로만 비교하며, 버전 값은 해시와 동등함이 보장되지 않습니다.", "", "## 질의 구간별 지연 비교", "", "| 종류 | 길이 | 결과 구간 | 질의 수 | 몽땅검색 p50 | Everything p50 | p50 비율 | 몽땅검색 p95 | Everything p95 | p95 비율 | 판정 |", "|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---|"]
    for (kind,length,band),items in sorted(groups.items()):
        kp=[x[3]["p50"] for x in items if x[3]["p50"] is not None]; ep=[x[4]["p50"] for x in items if x[4]["p50"] is not None]
        k95=[x[3]["p95"] for x in items if x[3]["p95"] is not None]; e95=[x[4]["p95"] for x in items if x[4]["p95"] is not None]
        # Aggregated p50/p95 calculated over per-query summaries.
        a,b=pct(kp,.5),pct(ep,.5); aa,bb=pct(k95,.5),pct(e95,.5)
        if len(items)<2 or len(kp)<2 or len(ep)<2: verdict="표본 부족"
        elif a is None or b is None: verdict="비교 불가"
        else: verdict="승" if a<b else "패" if a>b else "무승부"
        lines.append(f"| {kind} | {length} | {band} | {len(items)} | {a if a is not None else '—'} | {b if b is not None else '—'} | {ratio_fmt(a,b)} | {aa if aa is not None else '—'} | {bb if bb is not None else '—'} | {ratio_fmt(aa,bb)} | {verdict} |")
    lines += ["", "## 결과 정확도 불일치", "", "| 질의 | 몽땅검색 건수 | Everything 건수 | 상위 N 집합 일치율 |", "|---|---:|---:|---:|"]
    if discrepancies:
        for item,kc,ec,match in discrepancies: lines.append(f"| {qlabel(item[2])} | {kc if kc is not None else '—'} | {ec if ec is not None else '—'} | {match if match is not None else '측정 불가'} |")
    else: lines.append("| 불일치 없음 또는 상위 N 비교 자료 없음 | — | — | — |")
    lines += ["", "## 몽땅검색이 느린 질의", ""]
    if slow:
        for ratio,item in slow[:10]: lines.append(f"- {qlabel(item[2])}: p50이 Everything의 {ratio:.2f}배")
    else: lines.append("- 비교 가능한 지연 자료 없음")
    lines += ["", "## 오류·시간 초과", ""]
    lines.extend([f"- {qlabel(item[2])}: 오류 {err}, 시간 초과 {to}" for item,err,to in failures] or ["- 기록 없음"])
    lines += ["", "## 자원 요약", "", "| 실행 | 대상 | 상태 | 유휴/검색 후/최대 메모리·CPU·I/O | 측정 불가 사유 |", "|---:|---|---|---|---|"]
    for idx,d in enumerate(data_list,1):
        for r in resource_info(d):
            lines.append(f"| {idx} | {r.get('engine',r.get('role','대상 미상'))} | {r.get('status','unknown')} | WS peak {r.get('working_set_peak_bytes','—')}; private peak {r.get('private_peak_bytes','—')}; Δprivate {r.get('private_delta_bytes','—')}; CPU {r.get('cpu_delta_ms','—')} ms; I/O R/W {r.get('read_bytes_delta','—')}/{r.get('write_bytes_delta','—')} B | {r.get('unavailable_reason','—')} |")
    lines += ["", "질의별 리소스의 실행 단위·필드가 없는 자료는 질의당 비용 및 유휴 상주 비교를 계산할 수 없습니다. 원시 표본이 없는 경우 메모리 증가 기울기도 계산하지 않습니다.", "", "## 여러 실행 회귀 추적", ""]
    regs=regression(data_list)
    if regs:
        for h,ch,status,n,_,_ in regs: lines.append(f"- `{h[:16]}` 지연 변화 {ch*100:+.1f}% ({status}, 실행 {n}회)")
    else: lines.append("- 같은 질의 세트로 비교 가능한 다중 실행이 없습니다.")
    lines += ["", "## TODO 후보", ""]
    todo=[]
    for (kind,length,band),items in groups.items():
        kp=[x[3]["p95"] for x in items if x[3]["p95"] is not None]; ep=[x[4]["p95"] for x in items if x[4]["p95"] is not None]
        a,b=pct(kp,.5),pct(ep,.5)
        if len(items)>=2 and a is not None and b and a/b>=REGRESSION_RATIO: todo.append(f"{length}글 {kind} 질의 p95가 Everything의 {a/b:.1f}배 → 개선 후보")
    lines.extend(f"- {t}" for t in todo) if todo else lines.append("- 임계값을 넘은 구간이 없습니다.")
    lines += ["", "## 경고·한계", ""]
    lines.extend(f"- {w}" for w in warnings) if warnings else lines.append("- 누락 필드 경고 없음")
    lines += ["- 결과 수와 상위 N 일치율은 결과 정확도 지표이며 검색 문법의 완전한 동등성을 입증하지 않습니다.", "- 실행 간 비교는 공통 질의 해시와 관측된 설정이 일치하는 범위의 경보용 휴리스틱입니다.", "- 입력 JSON에 query_text가 없는 경우 보고서는 원문을 만들지 않으며 식별자는 종류·길이·해시만 표시합니다.", ""]
    return "\n".join(lines)

def main(argv=None):
    p=argparse.ArgumentParser(description="qms_bench JSON 분석기")
    p.add_argument("results",nargs="+",help="결과 JSON 경로")
    p.add_argument("--out",default="report.md",help="마크다운 보고서 경로")
    p.add_argument("--compare",help="비교 기준 JSON 경로")
    a=p.parse_args(argv)
    try:
        paths=([a.compare] if a.compare else [])+a.results
        loaded=[]; warnings=[]
        for path in paths:
            d,w=load_result(path); loaded.append(d); warnings.extend(w)
        report=render_report(loaded,warnings,paths)
        Path(a.out).write_text(report,encoding="utf-8")
    except (InputError,OSError) as exc:
        print(str(exc),file=sys.stderr); return 2
    print(f"보고서를 저장했습니다: {a.out}")
    if warnings:
        for w in warnings: print(f"경고: {w}",file=sys.stderr)
        return 1
    return 0
if __name__=="__main__": raise SystemExit(main())
