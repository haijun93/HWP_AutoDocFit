"""장·절·세부 띠, 인접 표 제목, 공유 문단 모양을 보존하는 개요 정렬."""
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile
from defusedxml import ElementTree as ET
from docfit_core import format_elements as fe
from docfit_core.document_structure import band, analyze, justify_overview
from tests.test_format_elements import HEADER,NS,_cell,_p,TITLE,OVERVIEW,classify


def table(values):
    return ET.fromstring(f'<hp:tbl {NS} rowCnt="1" colCnt="{len(values)}"><hp:tr>'+''.join(_cell(i,0,2,10000,[(4 if i==len(values)-1 else 0,v)]) for i,v in enumerate(values))+'</hp:tr></hp:tbl>')

def root_with(*items):
    r=ET.fromstring(f'<hs:sec {NS}/>')
    for value in items:
        if isinstance(value,str):r.append(ET.fromstring(f"<wrapper {NS}>"+value+"</wrapper>")[0])
        else:
            p=ET.fromstring(f'<hp:p {NS}><hp:run/></hp:p>');p[0].append(value);r.append(p)
    return r

class StructureTests(unittest.TestCase):
    def test_chapter_variants(self):
        for values,variant in [(['Ⅰ','','추진근거'],'A'),(['Ⅲ. 추진근거'],'B'),(['Ⅱ','추진근거'],'exception'),(['IV','','추진근거'],'A')]:
            found=band(table(values));self.assertEqual(found['role'],'chapter');self.assertEqual(found['variant'],variant)

    def test_section_square_number_and_detail(self):
        for values in [['1','','추진방향'],['1. 추진방향'],['\U000f02b2','추진방향'],['\U000f02b2 추진방향']]:
            found=band(table(values));self.assertEqual(found['role'],'section')
        self.assertEqual(band(table(['1-1','','세부 계획']))['role'],'detail')
        self.assertEqual(band(table(['❶ 세부 계획']))['role'],'detail')
        self.assertEqual(band(table(['\U000f02b2 계획']))['glyph_mapping'],'font-dependent')

    def test_false_positives(self):
        for values in [['1','금액','합계'],['단위','천원'],['Ⅰ'],['2026','1234']]:
            self.assertIsNone(band(table(values)),values)

    def test_prose_heading_typography_and_consecutive_levels(self):
        r=root_with(table(['Ⅰ','','장 제목']),table(['1','','절 제목']),table(['1-1','','세부 제목']),
                    _p(0,(0,'□ 소제목')),_p(0,(0,'ㅇ 본문')),_p(0,(0,'- 내용')))
        result=analyze([r],fe.HeaderIndex(ET.fromstring(HEADER)))
        nodes=result['nodes'];self.assertEqual([n['role'] for n in nodes],['chapter','section','detail','square','circle','dash'])
        self.assertEqual([n['parent_id'] for n in nodes],[None,0,1,2,3,4])
        r=root_with(table(['Ⅰ','','상위 장']),_p(0,(4,'① 절 제목')),_p(0,(0,'□ 하위 소제목')))
        nodes=analyze([r],fe.HeaderIndex(ET.fromstring(HEADER)))['nodes']
        self.assertEqual([n['role'] for n in nodes],['chapter','section','square'])

    def test_caption_is_adjacent_and_carries_style(self):
        data=ET.fromstring(f'<hp:tbl {NS} rowCnt="2" colCnt="2"><hp:tr>'+_cell(0,0,2,10000,[(0,'구분')])+_cell(1,0,2,10000,[(0,'금액')])+'</hp:tr><hp:tr>'+_cell(0,1,2,10000,[(0,'가')])+_cell(1,1,2,10000,[(0,'10')])+'</hp:tr></hp:tbl>')
        for title in ['&lt; 연도별 소요 예산(안) &gt;','[ 연도별 예산 ]','( 연도별 예산 )']:
            r=root_with(_p(0,(0,title)),data)
            captions=analyze([r],fe.HeaderIndex(ET.fromstring(HEADER)))['table_titles']
            self.assertEqual(len(captions),1);self.assertEqual(captions[0]['table_order'],1)
            self.assertEqual(captions[0]['typography']['size'],1500)
        r=root_with(_p(0,(0,'[ 예산 ]')),_p(0,(0,'')),data)
        self.assertEqual(analyze([r],fe.HeaderIndex(ET.fromstring(HEADER)))['table_titles'],[])

    def test_overview_justify_does_not_mutate_shared_styles(self):
        header=ET.fromstring(HEADER);overview=ET.fromstring(f'<wrapper {NS}>{OVERVIEW}</wrapper>')[0]
        p=overview.find('.//{*}p');p.set('paraPrIDRef','1')
        original=ET.tostring(header.find('.//{*}paraPr[@id="1"]'))
        self.assertGreater(justify_overview(header,overview),0)
        self.assertEqual(fe.HeaderIndex(header).para(p.get('paraPrIDRef'))['align'],'JUSTIFY')
        self.assertEqual(ET.tostring(header.find('.//{*}paraPr[@id="1"]')),original)
        self.assertEqual(justify_overview(header,overview),0)

    def test_analysis_and_review_nodes_include_new_elements(self):
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'sample.hwpx'
            root=root_with(table(['Ⅰ. 장 제목']),table(['1','','절 제목']))
            with ZipFile(path,'w') as z:
                z.writestr('mimetype','application/hwp+zip');z.writestr('Contents/header.xml',HEADER)
                z.writestr('Contents/section0.xml',ET.tostring(root))
            result=fe.analyze_format_elements(path,classify)
            self.assertIn('chapter_band_1',result['forms']);self.assertIn('section_band_3',result['forms'])
            self.assertIn('document_structure',result)
            self.assertTrue(any(p[0][0]=='form' and p[0][1]=='section_band_3' for p in fe.detail_nodes(result)))

    def test_summary_is_not_an_ancestor_of_chapter(self):
        r=root_with(ET.fromstring(f'<wrapper {NS}>{TITLE}</wrapper>')[0],ET.fromstring(f'<wrapper {NS}>{OVERVIEW}</wrapper>')[0],table(['Ⅰ','','장 제목']))
        nodes=analyze([r],fe.HeaderIndex(ET.fromstring(HEADER)),classify)['nodes']
        title=next(n for n in nodes if n['role']=='title')
        self.assertEqual(next(n for n in nodes if n['role']=='chapter')['parent_id'],title['id'])

    def test_existing_overview_is_justified_even_when_width_is_already_fitted(self):
        import runpy
        from unittest.mock import patch
        from tests.test_format_elements import SECTION
        ns=runpy.run_path(str(Path(__file__).resolve().parents[1]/'hwp-auto-docfit.py'),run_name='import_check')
        fn=ns['제목개요폭_hwpx_처리']
        with tempfile.TemporaryDirectory() as folder:
            source=Path(folder)/'source.hwpx';first=Path(folder)/'first.hwpx'
            second=Path(folder)/'second.hwpx';final=Path(folder)/'final.hwpx'
            with ZipFile(source,'w') as z:
                z.writestr('mimetype','application/hwp+zip');z.writestr('Contents/header.xml',HEADER)
                z.writestr('Contents/section0.xml',SECTION)
            with patch.dict(fn.__globals__,{'표준서식_사용':False}):
                fn(source,first,fn(source))
                with ZipFile(first) as z: contents={i.filename:z.read(i.filename) for i in z.infolist()}
                r=ET.fromstring(contents['Contents/section0.xml'])
                overview=r.findall('.//{*}tbl')[1]
                p=overview.find('.//{*}p');p.set('paraPrIDRef','1')
                contents['Contents/section0.xml']=ET.tostring(r)
                with ZipFile(second,'w') as z:
                    for name,data in contents.items():z.writestr(name,data)
                self.assertIn(1,fn(second)['Contents/section0.xml'])
                fn(second,final,fn(second))
                self.assertEqual(fn(final),{'Contents/section0.xml':[]})
                with ZipFile(final) as z:
                    h=ET.fromstring(z.read('Contents/header.xml'));r=ET.fromstring(z.read('Contents/section0.xml'))
                    p=r.findall('.//{*}tbl')[1].find('.//{*}p')
                    self.assertEqual(fe.HeaderIndex(h).para(p.get('paraPrIDRef'))['align'],'JUSTIFY')
