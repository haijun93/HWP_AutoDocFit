import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/conversion/conversion_workspace.dart';
import 'package:docfit_ui/main.dart';

class FakeConversionHost implements ConversionHost {
  final calls = <List<Object?>>[];
  @override
  final stateVersion = ValueNotifier<int>(0);

  @override
  List files = [];

  @override
  List results = [];

  @override
  Map failures = {};

  @override
  List conversionTargets = [
    {'format': 'hwpx', 'label': 'HWPX', 'enabled': true},
  ];

  @override
  bool locked = false;

  @override
  bool running = false;

  @override
  String statusText = '';

  @override
  double? progressValue;

  @override
  Future<void> command(String method, [List<Object?> args = const []]) async =>
      calls.add([method, ...args]);

  @override
  Future<void> convertTo(String format) async => calls.add(['convertTo', format]);

  @override
  Future<void> convertSingleFile(int index, String format) async =>
      calls.add(['start_single_file_conversion', index, format]);

  @override
  Future<void> addConversionFiles() async => calls.add(['add_files']);

  @override
  Map? hwpBulk;

  @override
  Future<Object?> apiCall(String method, List<Object?> args) async {
    calls.add([method, ...args]);
    if (method == 'convert_cell_bg') {
      return {
        'ok': true,
        'source': r'C:\docs\샘플.hwpx',
        'output': r'C:\docs\샘플 (표배경변환).hwpx',
        'converted_count': 3,
        'message': '표 칸 그림 3개를 셀 배경으로 변환하여 저장했습니다.',
      };
    }
    return {'ok': true};
  }
}

Future<void> _rightClick(WidgetTester tester, Finder target) async {
  final gesture = await tester.startGesture(
    tester.getCenter(target),
    kind: PointerDeviceKind.mouse,
    buttons: kSecondaryMouseButton,
  );
  await gesture.up();
  await tester.pumpAndSettle();
}

void main() {
  group('표 칸 그림 → 배경 변환 위젯 시험 (ConversionWorkspace)', () {
    testWidgets('파일 목록이 비었을 때 표 칸 그림을 배경으로 누르면 안내 다이얼로그 표시', (
      WidgetTester tester,
    ) async {
      final host = FakeConversionHost();
      host.files = [];

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: ConversionWorkspace(host: host)),
        ),
      );
      await tester.pumpAndSettle();

      // 버튼 존재 확인
      final btnFinder = find.byKey(const Key('conversionCellBg'));
      expect(btnFinder, findsOneWidget);
      expect(find.text('표 칸 그림을 배경으로'), findsOneWidget);

      // 클릭 시 안내 대화상자 표시
      await tester.tap(btnFinder);
      await tester.pumpAndSettle();

      expect(find.byKey(const Key('cellBgNoFileDialog')), findsOneWidget);
      expect(find.textContaining('변환할 HWPX 문서를 먼저 목록에 추가해 주세요.'), findsOneWidget);

      // 확인 눌러 닫기
      await tester.tap(find.byKey(const Key('cellBgNoFileConfirm')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('cellBgNoFileDialog')), findsNothing);
    });

    testWidgets('HWPX 파일이 있을 때 클릭 시 convert_cell_bg API 호출 및 결과 다이얼로그 표시', (
      WidgetTester tester,
    ) async {
      final host = FakeConversionHost();
      host.files = [
        {'name': '보고서.hwpx', 'path': r'C:\문서\보고서.hwpx', 'size': 1024 * 1024},
      ];

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: ConversionWorkspace(host: host)),
        ),
      );
      await tester.pumpAndSettle();

      final btnFinder = find.byKey(const Key('conversionCellBg'));
      await tester.tap(btnFinder);
      await tester.pumpAndSettle();

      // 백엔드 API 호출 검증
      expect(
        host.calls.any((call) => call.isNotEmpty && call[0] == 'convert_cell_bg'),
        isTrue,
      );

      // 결과 다이얼로그 노출 및 바뀐 칸 수 안내 검증
      expect(find.byKey(const Key('cellBgResultDialog')), findsOneWidget);
      expect(find.text('표 칸 그림 → 배경 변환 완료'), findsOneWidget);
      expect(find.text('바뀐 칸 수: 3개'), findsOneWidget);
      expect(find.textContaining(r'C:\docs\샘플 (표배경변환).hwpx'), findsOneWidget);

      // 확인 버튼 누르면 닫힘
      await tester.tap(find.byKey(const Key('cellBgResultCloseButton')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('cellBgResultDialog')), findsNothing);
    });

    testWidgets('HWPX 파일 우클릭 시 컨텍스트 메뉴에 표 칸 그림을 배경으로 표시 및 실행', (
      WidgetTester tester,
    ) async {
      final host = FakeConversionHost();
      host.files = [
        {'name': '테이블.hwpx', 'path': r'C:\문서\테이블.hwpx', 'size': 512 * 1024},
      ];

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: ConversionWorkspace(host: host)),
        ),
      );
      await tester.pumpAndSettle();

      // 파일 항목 우클릭
      await _rightClick(tester, find.byKey(const Key('conversionFile-0')));
      expect(find.text('표 칸 그림을 배경으로'), findsNWidgets(2));

      // 메뉴 항목 클릭 (컨텍스트 메뉴의 항목 선택)
      await tester.tap(find.text('표 칸 그림을 배경으로').last);
      await tester.pumpAndSettle();

      // 결과 다이얼로그 확인
      expect(find.byKey(const Key('cellBgResultDialog')), findsOneWidget);
      expect(find.text('바뀐 칸 수: 3개'), findsOneWidget);
    });
  });

  group('문서편집 화면 (Workspace) 표 칸 그림 변환 도구 위젯 시험', () {
    testWidgets('문서편집 addButtons에 표 칸 그림 버튼 렌더링 및 실행 결과 안내', (
      WidgetTester tester,
    ) async {
      final apiCalls = <List<Object?>>[];
      Future<Object?> fakeApi(String method, List<Object?> args) async {
        apiCalls.add([method, ...args]);
        if (method == 'get_state') {
          return {
            'mode': 'spacing',
            'files': [
              {'name': '기획서.hwpx', 'path': r'C:\work\기획서.hwpx', 'size': 2048},
            ],
            'count': 1,
            'status': '대기 중',
            'running': false,
            'range': {'enabled': false, 'start': '1', 'end': '1'},
            'conversion_targets': [],
          };
        }
        if (method == 'convert_cell_bg') {
          return {
            'ok': true,
            'source': r'C:\work\기획서.hwpx',
            'output': r'C:\work\기획서 (표배경변환).hwpx',
            'converted_count': 5,
            'message': '표 칸 그림 5개를 셀 배경으로 변환하여 저장했습니다.',
          };
        }
        return {'ok': true};
      }

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Workspace(api: fakeApi),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // 버튼 검증
      final toolBtn = find.byKey(const Key('cellBgToolButton'));
      expect(toolBtn, findsOneWidget);

      // 클릭 실행
      await tester.tap(toolBtn);
      await tester.pumpAndSettle();

      // 결과 다이얼로그 확인
      expect(find.byKey(const Key('workspaceCellBgResultDialog')), findsOneWidget);
      expect(find.text('바뀐 칸 수: 5개'), findsOneWidget);
      expect(find.textContaining(r'C:\work\기획서 (표배경변환).hwpx'), findsOneWidget);

      // 닫기
      await tester.tap(find.byKey(const Key('workspaceCellBgResultCloseButton')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('workspaceCellBgResultDialog')), findsNothing);
    });
  });
}
