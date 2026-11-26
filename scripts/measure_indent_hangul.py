"""합성 HWPX 를 새 한/글 인스턴스로 열어 Shift+Tab(ParagraphShapeIndentAtCaret) 값을 실측한다(INDENTW2).

- 합성 문서만 연다. 사용자가 열어 둔 한/글은 건드리지 않고, 이 스크립트가 띄운 프로세스만 닫는다.
- COM 잠금(docfit_core.save_guard.com_test_lock)을 잡고, 전체 시험 잠금(.full-test.lock)이 있으면 기다린다.
- 창은 최소화한다. 중간에 강제 종료하면 한/글 프로세스와 잠금이 남으니 끝까지 두거나 Ctrl+C 로 멈춘다.

사용:
    python scripts/measure_indent_hangul.py widths --cache <실측 저장.jsonl>              # 글자 폭 → 폭 표 갱신
    python scripts/measure_indent_hangul.py verify --cache <...> --output <결과.json>     # 전체 대조

측정 단위: 돌려주는 값은 HWPUNIT(1pt = 100). 한/글 COM ``ParaShape.Item("Indentation")`` 값을 ``COM_UNIT`` 로 나눈다.
"""
from __future__ import annotations

import contextlib
import json
import random
import sys
import tempfile
import time
import unicodedata
from dataclasses import asdict, dataclass
from pathlib import Path
from xml.sax.saxutils import escape, quoteattr

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "scripts"))

from docfit_core import font_widths as fw  # noqa: E402
from docfit_core import hwp_process  # noqa: E402
from docfit_core.fidelity.package import PackageSnapshot, build_package  # noqa: E402
from docfit_core.save_guard import DEFAULT_COM_LOCK_PATH, com_test_lock  # noqa: E402

TEMPLATE = ROOT / "resources" / "서식예시" / "기본 서식.hwpx"
COM_UNIT = 2   # ParaShape 여백 값 = HWPUNIT × 2 (실측)
FULL_TEST_LOCK = DEFAULT_COM_LOCK_PATH.with_name(".full-test.lock")
SECURITY_MODULE = ("FilePathCheckDLL", "HwpAutoDocFitSecurity")
FW_SPACE, NB_SPACE = fw.FW_SPACE, fw.NB_SPACE   # Case 문자열 안의 고정폭·묶음 빈칸
ALIGN_NAMES = ("LEFT", "JUSTIFY", "JUSTIFY_WORD")
LINE_WIDTH = 59528 - 2 * 8504   # 열린 합성 문서의 본문 폭(PageSetup 실측: 좌우 8504)
BATCH = 300                               # 문서 하나에 담는 문단 수


@dataclass(frozen=True)
class Style:
    face: str
    size: float = 15.0
    font_type: str = "TTF"
    ratio: int = 100
    spacing: int = 0
    rel_size: int = 100
    use_font_space: bool = False
    bold: bool = False
    slot_ratios: tuple = ()   # ((언어 칸, 장평), …) 언어 칸 판별 실측용
    slot_spacings: tuple = ()
    slot_faces: tuple = ()    # ((언어 칸, 글꼴), …) — 형식은 font_type

    def ratio_of(self, language: str) -> int:
        return dict(self.slot_ratios).get(language, self.ratio)

    slot_rels: tuple = ()

    def rel_of(self, language: str) -> int:
        return dict(self.slot_rels).get(language, self.rel_size)

    def spacing_of(self, language: str) -> int:
        return dict(self.slot_spacings).get(language, self.spacing)

    def face_of(self, language: str) -> str:
        return dict(self.slot_faces).get(language, self.face)

    def char_style(self) -> fw.CharStyle:
        return fw.CharStyle.uniform(self.face, self.size, font_type=self.font_type, ratio=self.ratio,
                                    spacing=self.spacing, rel_size=self.rel_size,
                                    use_font_space=self.use_font_space, bold=self.bold)


@dataclass(frozen=True)
class Case:
    runs: tuple               # ((글, Style), ...)
    offset: int               # Shift+Tab 을 실행할 문자 위치
    align: str = "LEFT"       # LEFT | JUSTIFY(한글 글자 나눔) | JUSTIFY_WORD(한글 어절 나눔)
    tag: str = ""
    left: int = 0             # 문단 왼쪽·오른쪽 여백(HWPUNIT)
    right: int = 0

    @property
    def text(self) -> str:
        return "".join(text for text, _ in self.runs)


def simple(prefix: str, style: Style, body: str = "가나다", align: str = "LEFT", tag: str = "") -> Case:
    return Case(((prefix + body, style),), len(prefix), align, tag)


# ── 합성 HWPX ────────────────────────────────────────────────

LANG_ATTRS = fw.LANGUAGES


def _all(value) -> str:
    return " ".join(f'{lang}="{value}"' for lang in LANG_ATTRS)


def _insert_items(xml: str, container: str, items: list[str], count_attr: str = "itemCnt") -> str:
    start = xml.index(f"<{container} ")
    end = xml.index(f"</{container}>", start)
    head_end = xml.index(">", start)
    head = xml[start:head_end]
    mark = f'{count_attr}="'
    a = head.index(mark) + len(mark)
    b = head.index('"', a)
    head = head[:a] + str(int(head[a:b]) + len(items)) + head[b:]
    return xml[:start] + head + xml[head_end:end] + "".join(items) + xml[end:]


def _text_xml(text: str) -> str:
    return escape(text).replace(FW_SPACE, "<hp:fwSpace/>").replace(NB_SPACE, "<hp:nbSpace/>")


def build_hwpx(cases: list[Case], target: Path) -> None:
    snapshot = PackageSnapshot.load(TEMPLATE)
    header = snapshot.read("Contents/header.xml").decode("utf-8")
    section = snapshot.read("Contents/section0.xml").decode("utf-8")

    faces: dict[tuple[str, str], int] = {}
    styles: dict[Style, int] = {}
    for case in cases:
        for _, style in case.runs:
            for lang in LANG_ATTRS:
                faces.setdefault((style.face_of(lang), style.font_type), 0)
            styles.setdefault(style, 0)

    font_start = header.index('<hh:fontface lang="HANGUL"')
    base_font_count = int(header[header.index('fontCnt="', font_start) + 9:].split('"', 1)[0])
    for index, key in enumerate(faces):
        faces[key] = base_font_count + index
    for lang in LANG_ATTRS:
        start = header.index(f'<hh:fontface lang="{lang.upper()}"')
        head_end = header.index(">", start)
        end = header.index("</hh:fontface>", start)
        count = int(header[header.index('fontCnt="', start) + 9:].split('"', 1)[0])
        if count != base_font_count:
            raise ValueError("템플릿의 언어별 글꼴 수가 다릅니다.")
        fonts = "".join(f'<hh:font id="{i}" face={quoteattr(face)} type="{kind}" isEmbedded="0"/>'
                        for (face, kind), i in faces.items())
        head = header[start:head_end].replace(f'fontCnt="{count}"', f'fontCnt="{count + len(faces)}"')
        header = header[:start] + head + header[head_end:end] + fonts + header[end:]

    first_char = header.count("<hh:charPr ")
    char_items = []
    for index, style in enumerate(styles):
        styles[style] = first_char + index
        refs = " ".join(f'{lang}="{faces[(style.face_of(lang), style.font_type)]}"' for lang in LANG_ATTRS)
        bold = "<hh:bold/>" if style.bold else ""
        ratios = " ".join(f'{lang}="{style.ratio_of(lang)}"' for lang in LANG_ATTRS)
        rels = " ".join(f'{lang}="{style.rel_of(lang)}"' for lang in LANG_ATTRS)
        spacings = " ".join(f'{lang}="{style.spacing_of(lang)}"' for lang in LANG_ATTRS)
        char_items.append(
            f'<hh:charPr id="{first_char + index}" height="{round(style.size * 100)}" textColor="#000000" '
            f'shadeColor="none" useFontSpace="{int(style.use_font_space)}" useKerning="0" symMark="NONE" '
            f'borderFillIDRef="2"><hh:fontRef {refs}/><hh:ratio {ratios}/>'
            f'<hh:spacing {spacings}/><hh:relSz {rels}/><hh:offset {_all(0)}/>{bold}'
            '<hh:underline type="NONE" shape="SOLID" color="#000000"/><hh:strikeout shape="NONE" color="#000000"/>'
            '<hh:outline type="NONE"/><hh:shadow type="NONE" color="#B2B2B2" offsetX="10" offsetY="10"/></hh:charPr>')
    header = _insert_items(header, "hh:charProperties", char_items)

    first_para = header.count("<hh:paraPr ")
    def margin(left, right):
        def block(scale):
            values = {"intent": 0, "left": left * scale, "right": right * scale, "prev": 0, "next": 0}
            return ("<hh:margin>" + "".join(f'<hc:{n} value="{v}" unit="HWPUNIT"/>' for n, v in values.items())
                    + '</hh:margin><hh:lineSpacing type="PERCENT" value="160" unit="HWPUNIT"/>')
        return ('<hp:switch><hp:case hp:required-namespace="http://www.hancom.co.kr/hwpml/2016/HwpUnitChar">'
                f'{block(1)}</hp:case><hp:default>{block(2)}</hp:default></hp:switch>')

    paras = {}
    para_items = []
    shapes = dict.fromkeys([("LEFT", 0, 0), *((case.align, case.left, case.right) for case in cases)])
    for index, (align, left, right) in enumerate(shapes):
        paras[(align, left, right)] = first_para + index
        para_items.append(
            f'<hh:paraPr id="{first_para + index}" tabPrIDRef="0" condense="0" fontLineHeight="0" snapToGrid="0" '
            f'suppressLineNumbers="0" checked="0"><hh:align horizontal="{align.split("_")[0]}" vertical="BASELINE"/>'
            '<hh:heading type="NONE" idRef="0" level="0"/><hh:breakSetting breakLatinWord="KEEP_WORD" '
            f'breakNonLatinWord="{"BREAK_WORD" if align.endswith("WORD") else "KEEP_WORD"}" widowOrphan="0" keepWithNext="0" keepLines="0" pageBreakBefore="0" '
            'lineWrap="BREAK"/><hh:autoSpacing eAsianEng="0" eAsianNum="0"/>' + margin(left, right) +
            '<hh:border borderFillIDRef="2" offsetLeft="0" offsetRight="0" offsetTop="0" offsetBottom="0" '
            'connect="0" ignoreMargin="0"/></hh:paraPr>')
    header = _insert_items(header, "hh:paraProperties", para_items)

    # 구역 설정(secPr)을 가진 첫 문단은 빈 문단으로 두고, 시험 문단은 1번부터다.
    first_p = section.index("<hp:p ")
    sec_start = section.index("<hp:secPr ")
    sec_end = section.index("</hp:secPr>") + len("</hp:secPr>")
    setup = section[sec_start:sec_end]

    def paragraph(para_id, runs):
        return (f'<hp:p id="0" paraPrIDRef="{para_id}" styleIDRef="0" pageBreak="0" columnBreak="0" merged="0">'
                f'{runs}</hp:p>')

    body = [paragraph(paras[("LEFT", 0, 0)], f'<hp:run charPrIDRef="0">{setup}</hp:run>')]
    for case in cases:
        runs = "".join(f'<hp:run charPrIDRef="{styles[style]}"><hp:t>{_text_xml(text)}</hp:t></hp:run>'
                       for text, style in case.runs)
        body.append(paragraph(paras[(case.align, case.left, case.right)], runs))
    section = section[:first_p] + "".join(body) + "</hs:sec>"

    target.write_bytes(build_package(snapshot, {
        "Contents/header.xml": header.encode("utf-8"),
        "Contents/section0.xml": section.encode("utf-8"),
        "Preview/PrvText.txt": b"synthetic",
    }))


# ── 한/글 ─────────────────────────────────────────────────

class Hangul:
    """이 스크립트가 띄운 새 한/글 하나."""

    def __init__(self):
        import pythoncom
        import win32com.client

        pythoncom.CoInitialize()
        before = hwp_process.hwp_pids()
        self.app = win32com.client.DispatchEx("HwpFrame.HwpObject")
        time.sleep(1.0)
        self.pids = hwp_process.hwp_pids() - before
        self.opened: set[str] = set()
        if len(self.pids) != 1:
            self.app = None
            raise RuntimeError("새 한/글 프로세스 하나를 특정하지 못해 중단합니다(기존 창을 건드리지 않음).")
        try:
            if not self.app.RegisterModule(*SECURITY_MODULE):
                raise RuntimeError("한/글 보안 모듈 등록 실패")
            self.minimize()
        except BaseException:
            self.quit()
            raise

    def minimize(self) -> None:
        """창을 최소화한다. 숨김 창은 명령 하나가 약 0.3초, 최소화 창은 약 0.01초였다(실측)."""
        import win32gui
        import win32process

        self.app.XHwpWindows.Item(0).Visible = True

        def shrink(window, _):
            if win32process.GetWindowThreadProcessId(window)[1] in self.pids and win32gui.IsWindowVisible(window):
                win32gui.ShowWindow(window, 6)   # SW_MINIMIZE
            return True

        win32gui.EnumWindows(shrink, None)

    def open(self, path: Path) -> None:
        if not self.app.Open(str(path), "HWPX", "forceopen:true"):
            raise RuntimeError(f"합성 문서를 열지 못했습니다: {path.name}")
        self.opened.add(path.name.casefold())

    def set_indent(self, value: int) -> None:
        action = self.app.CreateAction("ParagraphShape")
        params = action.CreateSet()
        params.SetItem("Indentation", value)
        action.Execute(params)

    def indent_at(self, para: int, units: int) -> dict:
        """앱의 ``문단_내어쓰기_적용`` 과 같은 순서로 잰다. 값은 COM 단위(원값)."""
        hwp = self.app
        hwp.SetPos(0, para, 0)
        hwp.HAction.Run("MoveParaBegin")
        begin = hwp.GetPos()
        hwp.HAction.Run("MoveLineEnd")
        line_end = hwp.GetPos()
        hwp.HAction.Run("MoveParaEnd")
        wrapped = line_end[2] < hwp.GetPos()[2]
        hwp.HAction.Run("MoveParaBegin")
        target = (begin[0], begin[1], begin[2] + units)
        if target[2] >= line_end[2]:
            return {"error": "outside_first_line", "wrapped": wrapped}
        values = []
        for _ in range(5):
            if hwp.SetPos(*target) is False or tuple(hwp.GetPos()) != target:
                return {"error": "setpos", "wrapped": wrapped}
            if hwp.HAction.Run("ParagraphShapeIndentAtCaret") is False:
                return {"error": "command", "wrapped": wrapped}
            value = int(hwp.ParaShape.Item("Indentation"))
            values.append(value)
            if value >= 0 or (len(values) > 1 and values[-2] == value):
                break
        return {"first": -values[0], "final": -values[-1], "wrapped": wrapped}

    def close_document(self) -> None:
        self.app.Clear(1)   # 저장하지 않고 비운다

    def quit(self) -> None:
        if self.app is None:
            return
        try:
            documents = self.app.XHwpDocuments
            foreign = []
            for index in range(documents.Count):
                name = Path(documents.Item(index).FullName or "").name.casefold()
                if name and name not in self.opened:
                    foreign.append(name)
            if foreign:
                print("경고: 이 인스턴스에 다른 문서가 열려 있어 닫지 않습니다.", file=sys.stderr)
                return
            self.app.Clear(1)
            self.app.Quit()
        finally:
            self.app = None
        deadline = time.monotonic() + 15
        while self.pids & hwp_process.hwp_pids() and time.monotonic() < deadline:
            time.sleep(0.5)
        for pid in self.pids & hwp_process.hwp_pids():
            import subprocess
            subprocess.run(["taskkill", "/PID", str(pid), "/F"], capture_output=True, check=False)


@contextlib.contextmanager
def hangul_session(wait_sec: float = 7200.0):
    deadline = time.monotonic() + wait_sec
    while True:
        while FULL_TEST_LOCK.exists():
            if time.monotonic() > deadline:
                raise TimeoutError(f"전체 시험 잠금 대기 시간 초과: {FULL_TEST_LOCK}")
            time.sleep(15)
        with com_test_lock("INDENTW2", "indent measure", timeout_sec=max(1.0, deadline - time.monotonic())):
            if FULL_TEST_LOCK.exists():
                continue   # 잠금을 잡는 사이 전체 시험이 시작됨 → 풀고 다시 기다림
            hangul = Hangul()
            try:
                yield hangul
            finally:
                hangul.quit()
            return


def position_units(text: str, offset: int) -> int:
    return len(text[:offset].encode("utf-16-le")) // 2


def _style_key(style: Style) -> list:
    values = asdict(style)
    if not values["slot_rels"]:
        del values["slot_rels"]
    return list(values.values())


def case_key(case: Case) -> str:
    key = [[[t, _style_key(st)] for t, st in case.runs], case.offset, case.align]
    if case.left or case.right:
        key += [case.left, case.right]
    return json.dumps(key, ensure_ascii=False, separators=(",", ":"))


def _load_cache(cache: Path) -> dict:
    if not cache.is_file():
        return {}
    rows = (json.loads(line) for line in cache.read_text(encoding="utf-8").splitlines() if line.strip())
    return {key: value for key, value in rows}


def run(cases: list[Case], cache: Path, batch: int = BATCH) -> list[dict]:
    """각 case 의 {first, final(COM 원값 = HWPUNIT×COM_UNIT), wrapped} 또는 {error}.

    끝난 값은 ``cache``(JSON Lines)에 문서 하나마다 덧붙이며, 다시 실행하면 없는 것만 잰다.
    """
    done = _load_cache(cache)
    pending = list({case_key(c): c for c in cases if case_key(c) not in done}.items())
    if pending:
        with tempfile.TemporaryDirectory(prefix="indentw2-") as folder, hangul_session() as hangul:
            for start in range(0, len(pending), batch):
                part = pending[start:start + batch]
                path = Path(folder) / f"indent_{start}.hwpx"
                build_hwpx([case for _, case in part], path)
                hangul.open(path)
                began = time.monotonic()
                lines = []
                for index, (key, case) in enumerate(part):
                    value = hangul.indent_at(index + 1, position_units(case.text, case.offset))
                    done[key] = value
                    lines.append(json.dumps([key, value], ensure_ascii=False))
                hangul.close_document()
                with cache.open("a", encoding="utf-8", newline="\n") as stream:
                    stream.write("".join(line + "\n" for line in lines))
                print(f"{start + len(part)}/{len(pending)} {time.monotonic() - began:.1f}s", flush=True)
    return [done[case_key(case)] for case in cases]


# ── 글자 폭 실측 → 폭 표 ─────────────────────────────────

MEASURE_SIZE = 50                       # pt. 한/글 격자(4 HWPUNIT)로 em 당 1250 단계
MEASURE_UNITS = MEASURE_SIZE * 100 // 4
HANGUL_SAMPLE = ("가나다라마바사아자차카타파하거너더러머버서어저처커터퍼허고노도로모보소오조초코토포호구누두루무부수우주추쿠투푸후"
                 "그는들를은의에이기리시지한및등함됨음임것위해년월일명원개건팀장과국청구동붙임끝제항목추진계획현황결과보고검토협조관련"
                 "사업예산지원대상기간방법내용")
PROBE = (
    "".join(map(chr, range(0x21, 0x7F))) + "".join(chr(c) for c in range(0xA1, 0x100) if c != 0xAD)
    + "‐‑‒–—―‖‘’‚“”„†‡•‣․‥…‧‰′″※‼‾⁃⁎⁑⁕℃℉№℡™Ω㎜㎝㎞㎡㎢㎥㎎㎏㎖ℓ㎾㏊㈜㈀㈁㉠㉡㉢㉣㉤㉥㉦㉧㉨㉩㉪㉫㉬㉭㉮㉯㉰㉱㉲㉳㉴㉵㉶㉷㉸㉹㉺㉻"
    + "ⅠⅡⅢⅣⅤⅥⅦⅧⅨⅩⅪⅫⅰⅱⅲⅳⅴⅵⅶⅷⅸⅹ←↑→↓↔↕↖↗↘↙⇒⇔⇐⇑⇓∀∂∃∇∈∋∏∑−∓∕∗∘∙√∝∞∠∥∧∨∩∪∫∴∵∼≒≠≡≤≥≪≫⊂⊃⊆⊇⊙⊥⋅⌒"
    + "①②③④⑤⑥⑦⑧⑨⑩⑪⑫⑬⑭⑮⑯⑰⑱⑲⑳⑴⑵⑶⑷⑸⑹⑺⑻⑼⑽⒈⒉⒊ⓐⓑⓒ❶❷❸❹❺❻❼❽❾❿➀➁➂➊➋➌➍➎➏➐➑➒➓➔➜➡➢➤"
    + "■□▢▣▤▥▦▧▨▩▪▫▬▭▲△▴▵▶▷▸▹►▻▼▽▾▿◀◁◂◃◄◅◆◇◈◉◊○◌◍◎●◐◑◘◙◦◯★☆☎☏☐☑☒☜☞♀♂♠♡♣♤♥♧♨♩♪♬♭✓✔✕✖✗✘✚✜✦✧✱✲✳❍❏❐❑❒❖❚❛❜"
    + "、。〃々〈〉《》「」『』【】〒〓〔〕〖〗～" + "".join(map(chr, range(0x3131, 0x318F)))
    + "（）［］｛｝：；，．？！＊＋－／＝＜＞｜￦｢｣･" + "αβγδεζηθλμπρστφχψωΑΒΓΔΘΛΠΣΦΨΩ€₩₤¥£漢字中大")


def _measure_chars(table: dict) -> dict[tuple[str, str], list[str]]:
    """(글꼴, 형식) → 실측할 글자. TTF 는 어느 글꼴 표에든 있는 비한글 글자 전부를 함께 잰다."""
    common = [c for c in PROBE + HANGUL_SAMPLE if unicodedata.category(c)[0] not in "CMZ"]
    union = {code for font in table["fonts"] if font["type"] == "TTF"
             for first, last, _ in font["ranges"] for code in range(first, last + 1)
             if not 0xAC00 <= code <= 0xD7A3 and unicodedata.category(chr(code))[0] not in "CMZ"}
    ttf = list(dict.fromkeys([*map(chr, sorted(union)), *common]))
    return {(font["face"], font["type"]): ttf if font["type"] == "TTF" else list(dict.fromkeys(common))
            for font in table["fonts"]}


def update_table(cache: Path, table_path: Path = fw.DATA_PATH) -> None:
    """글자 폭을 한/글에 재서 표에 넣는다. TTF 는 글꼴 파일과 다른 글자만, HFT 는 잰 글자 전부."""
    import make_font_width_table as maker

    table = json.loads(table_path.read_text(encoding="utf-8"))
    chars = _measure_chars(table)

    def case(face, kind, ch, bold=False):
        # 빈칸은 '글꼴에 어울리는 빈칸'을 켜야 글꼴의 빈칸 폭이 나온다.
        return Case(((ch + "가", Style(face, MEASURE_SIZE, kind, use_font_space=ch == " ", bold=bold)),), 1)

    keys = []
    for (face, kind), items in chars.items():
        if kind == "HFT":     # HFT 는 글꼴 빈칸과 굵게 폭도 잰다
            keys += [(face, kind, ch, bold) for ch in (" ", *items) for bold in (False, True)]
        else:                 # TTF 는 굵게일 때 글꼴 빈칸만 달라진다
            keys += [(face, kind, ch, False) for ch in items] + [(face, kind, " ", bold) for bold in (False, True)]
    results = dict(zip(keys, run([case(*key) for key in keys], cache)))

    def widths(face, kind, items, bold):
        found = {ord(ch): round(results[(face, kind, ch, bold)]["final"] / COM_UNIT / 4)
                 for ch in items if "final" in results[(face, kind, ch, bold)]}
        if kind == "HFT":     # 표본 음절의 폭이 모두 같으면 한글 음절 전체에 쓴다
            hangul = {found[ord(c)] for c in HANGUL_SAMPLE if ord(c) in found}
            if len(hangul) == 1:
                found.update(dict.fromkeys(range(0xAC00, 0xD7A4), hangul.pop()))
        return found

    for font in table["fonts"]:
        face, kind = font["face"], font["type"]
        items = chars[(face, kind)]
        if kind == "HFT":
            measured = widths(face, kind, (" ", *items), False)
            font.update(status="measured", units_per_em=MEASURE_UNITS, ranges=maker.compress(measured))
            font["bold"] = {"units_per_em": MEASURE_UNITS,
                            "ranges": maker.compress(widths(face, kind, (" ", *items), True))}
        else:
            measured = widths(face, kind, items, False)
            from_file = fw._Widths(font)
            differs = {}
            for code, units in measured.items():
                em = from_file.em(chr(code))
                if em is None or abs(int(MEASURE_UNITS * em + 0.5) - units) > 1:
                    differs[code] = units
            if differs:
                font["measured"] = {"units_per_em": MEASURE_UNITS, "ranges": maker.compress(differs)}
            else:
                font.pop("measured", None)
            space = widths(face, kind, (" ",), False)
            bold_space = widths(face, kind, (" ",), True)
            if bold_space != space:
                font["bold"] = {"units_per_em": MEASURE_UNITS, "ranges": maker.compress(bold_space)}
            else:
                font.pop("bold", None)
        print(font["face"], font["type"], len(measured), "실측 저장",
              len(font.get("measured", {}).get("ranges", [])), flush=True)
    table_path.write_text(maker.dumps(table), encoding="utf-8", newline="\n")
    fw._tables.cache_clear()


# ── 전체 대조 ───────────────────────────────────────────

TTF_FACES = ("휴먼명조", "함초롬바탕", "함초롬돋움", "HY헤드라인M", "맑은 고딕", "한컴돋움", "HY견고딕")
SIZES = (12, 13, 14, 15, 16, 17, 18, 20)
SHAPES = ((100, 0), (95, 0), (90, -5), (100, -10), (97, 3))      # (장평, 자간)
MARKERS = ("□", "ㅁ", "○", "ㅇ", "-", "·", "※", "1.", "가.", "(1)", "①", "•", "☞", "*")
PREFIXES = tuple(" " * lead + marker + " " for marker in MARKERS for lead in range(4)) + (
    "붙임 : 1. ", "붙임  1. ", "붙임 1. ", "   2. ", "       2. ")
BODIES = (
    "가나다라 마바사 아자차카 타파하 " * 8,
    "추진계획에따른세부실행방안마련 및 관계부서협조사항검토결과보고 " * 4,
    "구청 각 부서는 사업의 추진 현황과 향후 계획을 검토하여 기한 내에 제출하여 주시기 바랍니다. " * 3,
)


def grid() -> list[tuple[str, Case]]:
    """(묶음 이름, Case). 한 줄 본문은 "내용", 여러 줄은 BODIES."""
    cases: list[tuple[str, Case]] = []
    hft_faces = [f["face"] for f in json.loads(fw.DATA_PATH.read_text(encoding="utf-8"))["fonts"] if f["type"] == "HFT"]
    for face in TTF_FACES:
        for size in SIZES:
            for ratio, spacing in SHAPES:
                style = Style(face, size, ratio=ratio, spacing=spacing)
                cases += [("왼쪽·한 줄(TTF)", simple(p, style, "내용")) for p in PREFIXES]
    for face in hft_faces:
        for size in (12, 15, 20):
            for ratio, spacing in ((100, 0), (95, -5)):
                style = Style(face, size, "HFT", ratio, spacing)
                cases += [("왼쪽·한 줄(HFT)", simple(p, style, "내용")) for p in PREFIXES]
    layouts = [("양쪽·한 줄", "JUSTIFY", ["내용"]), ("왼쪽·여러 줄", "LEFT", BODIES),
               ("양쪽·여러 줄(글자 나눔)", "JUSTIFY", BODIES), ("양쪽·여러 줄(어절 나눔)", "JUSTIFY_WORD", BODIES)]
    for face, kind in [(f, "TTF") for f in TTF_FACES] + [("휴먼명조", "HFT"), ("신명 중명조", "HFT")]:
        for size in (13, 15, 17):
            style = Style(face, size, kind)
            for name, align, bodies in layouts:
                cases += [(name, simple(p, style, body, align)) for p in PREFIXES for body in bodies]
    for face in TTF_FACES:
        for size in (13, 15):
            for ratio, spacing in ((100, 0), (90, -5)):
                style = Style(face, size, ratio=ratio, spacing=spacing, use_font_space=True)
                cases += [("글꼴 빈칸 옵션", simple(p, style, "내용")) for p in PREFIXES]
    # 앞 빈칸·기호·본문의 글자 모양이 다른 문단(표준서식의 기호 굵게·글꼴 차이)
    heads = (Style("HY견고딕", 17, bold=True), Style("한컴돋움", 15), Style("휴먼명조", 14, ratio=95),
             Style("함초롬바탕", 13, spacing=-5), Style("신명 중명조", 15, "HFT"), Style("맑은 고딕", 12, bold=True))
    for index, marker_style in enumerate(heads):
        lead_style, body_style = heads[(index + 1) % len(heads)], heads[(index + 2) % len(heads)]
        for p in PREFIXES:
            marker = p.lstrip()
            lead = p[: len(p) - len(marker)]
            runs = tuple(run for run in ((lead, lead_style), (marker, marker_style), ("내용", body_style)) if run[0])
            cases.append(("글자 모양 혼합", Case(runs, len(p), "LEFT")))
    for face in TTF_FACES:
        style = Style(face, 15)
        for p in (FW_SPACE + "□ ", "ㅇ" + FW_SPACE, FW_SPACE * 2 + "- ", "※" + NB_SPACE,
                  NB_SPACE * 3 + "- ", "□" + NB_SPACE):
            cases.append(("고정폭·묶음 빈칸", simple(p, style, "내용")))
    # 긴 라벨(글자 수가 많아 글자당 오차가 쌓이는 경우)
    labels = ("  ㅇ 추진 기간 및 소요 예산(총괄) : ", "   - Project 2026 budget(KRW) 12,345,678 : ",
              "※ 관계부서 협조사항 및 향후 추진일정 검토 : ", "    * 항세기와 난세기 교차 예측 : ")
    for face, kind in [(f, "TTF") for f in TTF_FACES] + [("휴먼명조", "HFT"), ("신명 중명조", "HFT")]:
        for size in (12, 13, 15, 17):
            for ratio, spacing in ((100, 0), (95, 0), (90, -5), (97, -3), (93, -7), (80, 5), (110, 10)):
                style = Style(face, size, kind, ratio, spacing)
                cases += [("긴 라벨", simple(p, style, "내용")) for p in labels]
    # 양쪽 정렬 규칙 검증: 규칙을 정할 때 쓰지 않은 본문·여백·크기·장평·자간
    holdout = (
        "2026년도 지역사회 통합돌봄(Care) 사업을 추진함에 있어, 각 동 주민센터는 「대상자 발굴」 결과를 10월 31일까지 "
        "제출하여 주시기 바라며, 기타 문의사항은 담당자(02-3153-0000)에게 연락 바랍니다. " * 2,
        "마포구는 AI-based document automation 시스템 도입을 검토 중이며, 1,234,567원의 예산(국비 50%, 시비 25%)을 "
        "편성할 예정임. 상세 내용은 붙임을 참고하여 주시기 바람. " * 2,
        "관련 부서와 협의한 결과 다음과 같이 추진하고자 하오니 검토 후 의견을 회신하여 주시기 바랍니다 " * 3,
    )
    margins = ((0, 0), (0, 3000), (2000, 7000), (1000, 12500))
    for face, kind in [(f, "TTF") for f in TTF_FACES] + [("휴먼명조", "HFT"), ("신명 중명조", "HFT")]:
        for size, ratio, spacing in ((12, 100, 0), (14, 95, -5), (16, 97, -3), (15, 90, -10), (13, 105, 3)):
            style = Style(face, size, kind, ratio, spacing)
            for number, prefix in enumerate(("  ㅇ ", "    - ", "□ ", "붙임 : 1. ", "   ※ ")):
                left, right = margins[(number + size) % len(margins)]
                for body in holdout:
                    for align, unit in (("JUSTIFY", "글자"), ("JUSTIFY_WORD", "어절")):
                        cases.append((f"양쪽·검증({unit} 나눔)",
                                      Case(((prefix + body, style),), len(prefix), align, left=left, right=right)))
    return cases + random_cases() + random_cases(seed=INDEPENDENT_SEED, label="독립 검증")


INDEPENDENT_SEED = 7121   # 규칙을 모두 확정한 뒤 한 번만 잰 검증 묶음


def random_cases(count: int = 4000, seed: int = 20261126, label: str = "무작위 검증") -> list[tuple[str, Case]]:
    """무작위 조합(고정 시드). 글꼴·크기·장평·자간·접두·본문·정렬·여백·글꼴 빈칸·굵게를 섞는다."""
    rng = random.Random(seed)
    table = json.loads(fw.DATA_PATH.read_text(encoding="utf-8"))
    faces = [(f["face"], f["type"]) for f in table["fonts"]]
    markers = (*MARKERS, "**", "◇", "▶", "◦", "▪", "–", "∙", "ㆍ", "➊", "㉠", "[가]", "가)", "1)", "1-1.", "③",
               "➜", "★", "(가)", "●", "■")
    words = ("마포구", "주민센터", "예산", "추진", "현황", "및", "향후", "계획을", "검토하여", "2026년", "10월", "AI",
             "system", "(총괄)", "「관련 법령」", "1,234,567원", "50%", "협조하여", "주시기", "바랍니다.", "문의:",
             "02-3153-0000", "안전점검", "사회적약자지원서비스체계구축", "등", "의", "’26~’30년", "·", "※", "→")
    cases = []
    for number in range(count):
        mixed = rng.random() < 0.25

        def style():
            face, kind = rng.choice(faces[:7] if rng.random() < 0.7 else faces)
            return Style(face, rng.choice((10, 11, 12, 12.5, 13, 14, 15, 16, 17, 18, 20, 22)), kind,
                         rng.choice((100, 100, rng.randint(70, 120))), rng.choice((0, 0, rng.randint(-15, 10))),
                         rng.choice((100, 100, 100, 90, 110)), rng.random() < 0.15, rng.random() < 0.2)

        main = style()
        lead = " " * rng.randint(0, 6)
        marker = rng.choice(markers) + " " * rng.choice((1, 1, 1, 2))
        if rng.random() < 0.15:
            marker += rng.choice(("추진 기간 : ", "(방식) : ", "일 시: "))
        body = " ".join(rng.choice(words) for _ in range(rng.choice((2, 60, 60))))
        align = rng.choice(ALIGN_NAMES)
        left, right = rng.choice((0, 0, 500, 1500, 3333)), rng.choice((0, 0, 2001, 6000, 11000))
        if mixed:
            runs = tuple(run for run in ((lead, style()), (marker, style()), (body, main)) if run[0])
        else:
            runs = ((lead + marker + body, main),)
        group = label + "(" + {"LEFT": "왼쪽", "JUSTIFY": "양쪽·글자", "JUSTIFY_WORD": "양쪽·어절"}[align] + ")"
        cases.append((group, Case(runs, len(lead + marker), align, left=left, right=right)))
    return cases


def predicted(case: Case):
    return fw.measure_runs([(text, style.char_style()) for text, style in case.runs], case.offset)


def predicted_justified(case: Case):
    return fw.justified_indent([(text, style.char_style()) for text, style in case.runs], case.offset,
                               LINE_WIDTH - case.left - case.right, korean_by_word=case.align == "JUSTIFY_WORD")


def verify(cache: Path, output: Path) -> None:
    """전체 대조를 실측해 ``output``(JSON)에 case 별 실측·계산·현행 계산(HWPUNIT)을 적는다."""
    from docfit_core.xml_hanging_indent import prefix_width_hwpunit

    items = grid()
    results = run([case for _, case in items], cache)
    rows = []
    for (group, case), result in zip(items, results):
        style = case.runs[0][1]
        width = predicted(case) if case.align == "LEFT" else predicted_justified(case)
        plain = predicted(case)
        rows.append({
            "group": group, "face": style.face, "type": style.font_type, "size": style.size, "ratio": style.ratio,
            "spacing": style.spacing, "prefix": case.text[: case.offset], "align": case.align,
            "body": case.text[case.offset:][:12], "wrapped": result.get("wrapped"), "error": result.get("error"),
            "hangul": None if "final" not in result else result["final"] / COM_UNIT,
            "first": None if "first" not in result else result["first"] / COM_UNIT,
            "table": None if width is None else width.width,
            "plain": None if plain is None else plain.width,
            "left": case.left, "right": case.right,
            "exact": None if width is None else width.exact,
            "current": prefix_width_hwpunit(case.text, case.offset, size_pt=style.size, ratio=style.ratio,
                                            spacing=style.spacing),
        })
    output.write_text(json.dumps(rows, ensure_ascii=False), encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    import argparse

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("command", choices=("widths", "verify"))
    parser.add_argument("--cache", type=Path, required=True, help="실측값 저장 파일(JSON Lines, 이어서 측정)")
    parser.add_argument("--output", type=Path, help="verify 결과 JSON")
    args = parser.parse_args(argv)
    if args.command == "widths":
        update_table(args.cache)
    else:
        if args.output is None:
            parser.error("verify 에는 --output 이 필요합니다.")
        verify(args.cache, args.output)
    return 0


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    raise SystemExit(main())
