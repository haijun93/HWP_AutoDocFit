"""Compatibility exports for formatter definitions split by feature.

Raw exports retain this module's globals for existing patch targets. Runtime
exports are still rebound by bind_function into each invocation's RunContext.
"""
from docfit_engine.runtime import bind_function, bind_class, _plain_bind
from . import formatter_utils
from . import formatter_analysis
from . import formatter_tables
from . import formatter_apply
from . import formatter_indent
from . import formatter_text

_TITLES_SYMBOLS = (
    '개요붙임_원본자료', '붙임_유형판별', '붙임_대상찾기', '중제목_글칸들', '중제목_유형판별', '중제목_대상찾기',
    '중제목_원본자료', '_구역_본문폭', '_예정_좌우여백', '_최종_본문폭', '_빈_문단인가', '_문단사이_빈줄_삭제',
    '_문단_본문글', '_서식표_뒤_빈줄_삭제', '_서식표_가로맞춤', '_중제목_번호굵게_적용', '중제목_표서식_복사',
    '_중제목_글자치수', '_중제목_문단폭', '_중제목_칸폭_맞춤', '개요_표인가', '제목_xml이름', '제목_자식',
    '제목_셀들', '제목_문단들', '제목_문자열', '_한칸_제목글인가', '_기타_제목표인가', '_한칸_제목표인가',
    '제목_유형판별', '제목_대상찾기', '서식표_종류판별', '_서식표_예시', '제목2행1열_원본자료', '제목_원본자료',
    '제목_참조병합', '날짜칸인가', '제목_날짜칸_한줄', '제목_표서식_복사', '제목_문단_가운데정렬', '제목_괄호부연_축소',
    '제목_hwpx_처리', '_제목_임시hwpx_저장', '이전_작업_임시폴더_정리', '_제목_임시폴더_정리', '서식구조_조사',
    '제목4종_현재문서_조사', '제목4종_현재문서_적용', '붙임_hwpx_처리', '중제목_hwpx_처리', '제목_부제_크기_해석',
    '제목_부제_크기_반영', '제목_담당자_글_반영', '_설정_담당자_글인가', '_서식표_기준', '제목_부제_크기',
    '_칸_글_지우기', '_칸_날짜_현행화', '_빈칸_서식_참조_변환', '_칸_글_넣기', '서식표_생성',
    '_제목개요_빈줄_정리', '_서식표_글', '_서식표_글_확인', '라벨_서식표_적용', '_문서_표번호',
    '_서식표_번호_부여', '제목개요폭_hwpx_처리', '상자서식_hwpx_처리', '쪽번호_hwpx_처리',
    '붙임2종_현재문서_조사', '제목붙임_선행적용', '정밀표_선행적용', '붙임2종_현재문서_적용',
)


def _install_titles(*, _docfit_namespace, _docfit_symbol, bind_function):
    from . import formatter_titles
    return formatter_titles._install(
        _docfit_namespace=_docfit_namespace,
        _docfit_symbol=_docfit_symbol,
        bind_function=bind_function,
    )


_UNIFY_SYMBOLS = (
    '_서식통일_문두요소_범위', '_서식통일_글자모양', '_서식통일_공백제거', '_서식통일_HWPX_분석',
    '_서식통일_run_text', '_서식통일_XML문단', '_서식통일_위치텍스트', '_서식통일_음영값',
    '서식통일_표본', '_서식통일_그룹키', '_서식통일_현재_HWPX', '_서식통일_불일치_구간',
    '_서식통일_부연괄호_불일치_구간', '_서식통일_굵기_불일치_구간', '서식통일_대표',
    '_서식통일_표지인가', '_서식통일_제목표_표본', '_서식통일_체계경계', '_서식통일_참고표_이웃대표_보완',
    '_서식통일_대표값_사용자검토', '_서식통일_적용영역_사용자검토', '서식통일_결과창_반영',
    '_결과창_띄우는가', '_서식통일_최종결과_사용자확인', '_서식통일_미확정_항목',
    '_서식통일_미확정_빨간표시', '_서식통일_문단앞간격_적용', '_서식통일_내어쓰기_필요',
    '_서식통일_내어쓰기_상태', '_서식통일_문단서식_적용', '_서식통일_글자요소_적용',
    '_서식통일_색값', '_서식통일_색_적용', '_서식통일_문단내어쓰기_적용', '_서식통일_문단자간_조정',
    '_서식통일_글꼴크기만', '_서식통일_글꼴크기_대표', '_서식통일_XML_조사', '_서식통일_COM_조사',
    '서식통일_전체_적용', '서식통일_자간조정_반영', '서식통일_문장_마무리', '_서식통일_마무리_위치로',
    '서식통일_보류자간_재조정', '_서식통일_표모형', '서식통일_표_전체_적용',
)


def _install_unify(*, _docfit_namespace, _docfit_symbol, bind_function):
    from . import formatter_unify
    return formatter_unify._install(
        _docfit_namespace=_docfit_namespace,
        _docfit_symbol=_docfit_symbol,
        bind_function=bind_function,
    )


_SPLIT_INSTALLERS = {}
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_utils.SYMBOLS, formatter_utils._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_analysis.SYMBOLS, formatter_analysis._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(_TITLES_SYMBOLS, _install_titles))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_tables.SYMBOLS, formatter_tables._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_apply.SYMBOLS, formatter_apply._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_indent.SYMBOLS, formatter_indent._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(_UNIFY_SYMBOLS, _install_unify))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_text.SYMBOLS, formatter_text._install))


def _export_split_function(function, namespace):
    # A raw re-export must look up dependencies through formatter.X, so patches
    # of that path continue to affect calls across feature module boundaries.
    exported = _plain_bind(function, globals())
    exported.__qualname__ = function.__qualname__
    globals()[function.__name__] = exported
    return exported


def _bind_split_function(function, namespace):
    return bind_function(_export_split_function(function, namespace), namespace)


def __getattr__(name):
    if name == "formatter_titles":
        from . import formatter_titles
        globals()["formatter_titles"] = formatter_titles
        return formatter_titles
    if name == "formatter_unify":
        from . import formatter_unify
        globals()["formatter_unify"] = formatter_unify
        return formatter_unify
    installer = _SPLIT_INSTALLERS.get(name)
    if installer is None:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    return installer(_docfit_namespace=globals(), _docfit_symbol=name,
                     bind_function=_export_split_function)


def _install(*, _docfit_namespace, _docfit_symbol):
    installer = _SPLIT_INSTALLERS.get(_docfit_symbol)
    if installer is not None:
        return installer(_docfit_namespace=_docfit_namespace, _docfit_symbol=_docfit_symbol,
                         bind_function=_bind_split_function)
    raise KeyError(_docfit_symbol)
