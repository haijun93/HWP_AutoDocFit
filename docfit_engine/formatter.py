"""Compatibility exports for formatter definitions split by feature.

Raw exports retain this module's globals for existing patch targets. Runtime
exports are still rebound by bind_function into each invocation's RunContext.
"""
from docfit_engine.runtime import bind_function, bind_class, _plain_bind
from . import formatter_utils
from . import formatter_analysis
from . import formatter_tables
from . import formatter_indent

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


_TEXT_SYMBOLS = (
    '괄호_안쪽', '대괄호인가', '괄호_문두_라벨인가', '괄호_뒤_콜론인가', '세트후속_범위괄호_라벨인가',
    '문단_범위_선택', '선택범위_현재크기_pt', '괄호_축소량_pt', '대괄호_축소_제외', '괄호_텍스트_크기_축소_현재선택',
    '문두_라벨_굵게_제외_문단인가', '항목기호_키', '일관성_굵게_머리말_범위', '문두_라벨_굵게_선반영',
    '괄호_및_라벨_텍스트_문단_처리', '라벨괄호_계획', '라벨괄호_hwpx_처리', '표안_문단_구분', '공백규칙_글변환',
    '문장부호뒤공백_글변환', '글규칙_hwpx_처리', '현재문서_XML단계_적용', '괄호_텍스트_크기_축소_전체_적용',
    '텍스트_hwpx로_변환', '라벨블록_한글삽입', 'AI답변_개조식_변환', '텍스트_hwpx_단독변환', '외부문서_hwpx로_변환',
    'PDF_서식_이식', '_박스그림표_한글표_삽입', '_박스그림_줄인가', '_박스그림_남은_문단', '_박스그림표_교체',
    '박스그림_전체_적용', '저장결과_규칙검수',
)


def _install_text(*, _docfit_namespace, _docfit_symbol, bind_function):
    from . import formatter_text
    return formatter_text._install(
        _docfit_namespace=_docfit_namespace,
        _docfit_symbol=_docfit_symbol,
        bind_function=bind_function,
    )


_APPLY_SYMBOLS = (
    '_표준서식_문단위간격_표', '기본_계층_스타일', '표준서식_문단위간격_찾기', '기호글꼴_적용', '표글꼴_적용',
    '복사_문단모양_적용', '계층_추가서식_값', '복사_간격_규칙_적용', '계층_추가서식_적용', '빈줄서식_hwpx_처리',
    '개요첫칸_hwpx_처리', '강조낱말_hwpx_처리', '_문단_일반글', '_문단_준말_판별', '_문단_준말_제외사유',
    '_문단_글_바꾸기', '준말_hwpx_처리', '연결부호탭_hwpx_처리', '콜론라벨_hwpx_처리', '준말_사용표_만들기',
    '준말_선행적용', '_위첨자_글자모양', '_문단_별표_대상', '별표위첨자_hwpx_처리', '별표위첨자_선행적용',
    '_한글_글꼴표', '_글꼴_확보', '_글자모양_글꼴크기', '_본문기호_글꼴', '붙임글꼴_hwpx_처리',
    '글자서식_선행적용', '문자모양_적용_현재선택', '_다른_글꼴형식으로_재실행', '문단_줄간격_적용_현재선택', 'HwpUnit_pt',
    'HwpUnit_mm', '문단_위간격_적용_현재선택', '문단_아래간격_pt_현재문단', '문단_아래간격_적용_현재선택', '_mm_hwpunit',
    '_현재_쪽여백', '_표준_또는_원본여백', '페이지_여백_설정', '용지_다르면_알림', '들여쓰기_공백_맞추기',
    '표준서식_기호규칙_찾기', '본문형_일반문장인가', '_사용자서식인가', '알파_일괄서식_가능', '사용자서식_XML추가값',
    '_GDI_글꼴인가', '_알파_글꼴형식', '_알파_라벨_굵게범위', '표준서식_hwpx_처리', '_알파_한줄문단인가',
    '표준서식_문단_처리', '빈문단_분류', '항목기호문장_사이_빈문단_찾기', '본문_컨트롤_문단번호', '항목기호문장_사이_빈줄_삭제',
    '글꼴형식_모으기', '글꼴형식_등록', '표준서식_전체_적용', '표준서식_선행_적용',
)


def _install_apply(*, _docfit_namespace, _docfit_symbol, bind_function):
    from . import formatter_apply
    return formatter_apply._install(
        _docfit_namespace=_docfit_namespace,
        _docfit_symbol=_docfit_symbol,
        bind_function=bind_function,
    )


_SPLIT_INSTALLERS = {}
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_utils.SYMBOLS, formatter_utils._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_analysis.SYMBOLS, formatter_analysis._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(_TITLES_SYMBOLS, _install_titles))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_tables.SYMBOLS, formatter_tables._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(_APPLY_SYMBOLS, _install_apply))
_SPLIT_INSTALLERS.update(dict.fromkeys(formatter_indent.SYMBOLS, formatter_indent._install))
_SPLIT_INSTALLERS.update(dict.fromkeys(_UNIFY_SYMBOLS, _install_unify))
_SPLIT_INSTALLERS.update(dict.fromkeys(_TEXT_SYMBOLS, _install_text))


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
    if name == "formatter_text":
        from . import formatter_text
        globals()["formatter_text"] = formatter_text
        return formatter_text
    if name == "formatter_apply":
        from . import formatter_apply
        globals()["formatter_apply"] = formatter_apply
        return formatter_apply
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
