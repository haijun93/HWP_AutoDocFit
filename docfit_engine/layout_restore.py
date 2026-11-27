"""쪽 간격 시도의 원복 확인(E07).

원복은 한 곳이 실패해도 나머지를 모두 시도하고, 다시 읽어 원래 값과 다르면 필수 수정 실패
(edit_failures)로 올려 결과 저장을 막는다.
"""
from docfit_engine.edit_failures import fail_required_edit


def restore_each(items, restore, label):
    """items를 하나씩 restore(item)한다. 예외이거나 False면 실패. 모두 시도한 뒤 실패를 올린다."""
    failed, cause = 0, None
    for item in items:
        try:
            if restore(item) is False:
                failed += 1
        except Exception as error:
            failed += 1
            cause = cause or error
    if failed:
        fail_required_edit(f'{label} 원복을 확인하지 못했습니다({failed}곳).', cause)


def restore_all(steps):
    """서로 독립인 원복 단계를 모두 실행하고 첫 오류를 다시 올린다."""
    first = None
    for step in steps:
        try:
            step()
        except Exception as error:
            first = first or error
    if first is not None:
        raise first


def read_text_colors(hwp, hwp_run, start, end, select, next_pos, cancelled):
    """[start, end) 글자색을 글자마다 읽어 [(시작, 끝, (색,))]로 보관한다. 중단·읽기 실패면 None."""
    runs, pos = [], start
    try:
        while pos[2] < end[2]:
            if cancelled():
                return None
            nxt = next_pos(pos)
            if not nxt or not pos[2] < nxt[2] <= end[2]:
                return None
            select(pos, nxt)
            pset = hwp.HParameterSet.HCharShape
            hwp.HAction.GetDefault('CharShape', pset.HSet)
            value = (int(pset.TextColor),)
            if runs and runs[-1][2] == value:
                runs[-1] = (runs[-1][0], nxt, value)
            else:
                runs.append((pos, nxt, value))
            pos = nxt
        return runs
    except Exception:
        return None
    finally:
        hwp_run('Cancel')


def write_text_colors(hwp, hwp_run, runs, select):
    for start, end, (color,) in runs:
        select(start, end)
        act = hwp.CreateAction('CharShape')
        pset = act.CreateSet()
        pset.SetItem('TextColor', color)
        if act.Execute(pset) is False:
            raise RuntimeError('글자색 적용 실패')
    hwp_run('Cancel')


def restore_char_values(hwp, hwp_run, records, apply, select, next_pos, fields, label):
    """records(바꾼 순서대로의 [(시작, 끝, fields 값)] 목록)의 글자 값을 되돌리고 글자마다 다시 읽어 확인한다.

    같은 글자가 여러 번 담겼으면 처음 담긴 값이 원래 값이다: 쓰기는 역순, 확인은 순서대로 처음 값만 본다.
    apply(runs, 0), select(시작, 끝), next_pos(위치)는 단어 모드의 적용·선택·다음 글자 함수다.
    """
    for runs in reversed(records):
        for run in runs:
            try:
                apply([run], 0)
            except Exception:
                pass                  # 적용 뒤 예외일 수 있어 결과는 다시 읽어 판단한다.
    seen = set()

    def matches(run):
        pos, end, values = run
        same = True
        while pos[2] < end[2]:
            nxt = next_pos(pos)
            if not nxt or not pos[2] < nxt[2] <= end[2]:
                return False
            if pos not in seen:
                seen.add(pos)
                select(pos, nxt)
                pset = hwp.HParameterSet.HCharShape
                hwp.HAction.GetDefault('CharShape', pset.HSet)
                same = tuple(int(getattr(pset, key)) for key in fields) == tuple(values) and same
            pos = nxt
        return same

    try:
        restore_each([run for runs in records for run in runs], matches, label)
    finally:
        hwp_run('Cancel')


def line_spacing_matches(hwp, pos, kind, value):
    hwp.SetPos(*pos)
    if tuple(hwp.GetPos()[:2]) != tuple(pos[:2]):
        return False
    pset = hwp.HParameterSet.HParaShape
    hwp.HAction.GetDefault('ParagraphShape', pset.HSet)
    return (int(pset.LineSpacingType), int(pset.LineSpacing)) == (kind, value)


def write_line_spacing(hwp, pos, kind, value):
    hwp.SetPos(*pos)
    act = hwp.CreateAction('ParagraphShape')
    pset = act.CreateSet()
    pset.SetItem('LineSpacingType', kind)
    pset.SetItem('LineSpacing', value)
    return act.Execute(pset) is not False


def confirm_line_spacing(hwp, backup):
    """backup [(위치, 형식, 값)]과 다른 문단은 다시 쓰고, 그래도 다르면 실패."""
    def restore(item):
        try:
            if line_spacing_matches(hwp, *item):
                return True
        except Exception:
            pass                      # 읽지 못했으면 다시 쓰고 확인한다.
        try:
            write_line_spacing(hwp, *item)
        except Exception:
            pass                      # 적용 뒤 예외일 수 있어 결과는 다시 읽어 판단한다.
        return line_spacing_matches(hwp, *item)

    restore_each(backup, restore, '문단 줄간격')


def confirm_margin(hwp, hwp_run, pos, key, value, read):
    """문단 위·아래 간격(pt)이 원래 값과 정확히 같은지 확인하고, 다르면 원시 값으로 다시 쓴다.

    pt를 PointToHwpUnit으로 바꾸면 소수가 잘려 1 HWPUNIT 작아질 수 있다(실측: 아래 간격 29 -> 28).
    COM 문단 간격은 pt의 200배다. key는 'PrevSpacing' 또는 'NextSpacing'.
    """
    def matches():
        hwp.SetPos(*pos)
        current = read()
        return current is not None and abs(current - value) < 0.001

    if matches():
        return True
    try:
        hwp.SetPos(*pos)
        hwp_run('MoveParaBegin')
        hwp_run('MoveSelParaEnd')
        act = hwp.CreateAction('ParagraphShape')
        pset = act.CreateSet()
        pset.SetItem(key, int(round(value * 200)))
        act.Execute(pset)
        hwp_run('Cancel')
    except Exception:
        pass                          # 결과는 다시 읽어 판단한다.
    return matches()


def read_page_break(hwp, pos):
    """'문단 앞에서 쪽 나눔' 값(0/1). 읽지 못하면 None."""
    try:
        hwp.SetPos(*pos)
        pset = hwp.HParameterSet.HParaShape
        hwp.HAction.GetDefault('ParagraphShape', pset.HSet)
        return int(bool(int(pset.PagebreakBefore or 0)))
    except Exception:
        return None
