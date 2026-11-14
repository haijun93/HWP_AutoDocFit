import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:docfit_ui/features/settings/search_service_card.dart';

void main() {
  testWidgets('초기 상태가 꺼짐일 때 정상 표시된다', (tester) async {
    Future<Object?> fakeApi(String method, List<Object?> args) async {
      expect(method, 'kfind_service');
      expect(args, ['status']);
      return {
        'ok': true,
        'status': 'stopped',
        'status_text': '꺼짐',
      };
    }

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SearchServiceCard(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('검색 서비스'), findsOneWidget);
    expect(find.text('지금 상태: 꺼짐'), findsOneWidget);
    expect(find.textContaining('UAC'), findsOneWidget);
    expect(find.text('관리자 서비스로 설치'), findsOneWidget);
    expect(find.text('관리자 서비스 제거'), findsOneWidget);
  });

  testWidgets('관리자 서비스 설치 클릭 시 API 호출되고 성공하면 상태가 바뀐다', (tester) async {
    final calls = <List<Object?>>[];
    Future<Object?> fakeApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (args.isNotEmpty && args[0] == 'status') {
        return {
          'ok': true,
          'status': 'user',
          'status_text': '사용자 모드 실행 중',
        };
      }
      if (args.isNotEmpty && args[0] == 'install') {
        return {
          'ok': true,
          'message': '설치 완료',
          'status': 'service',
          'status_text': '관리자 서비스',
        };
      }
      return {'ok': false};
    }

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SearchServiceCard(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('지금 상태: 사용자 모드 실행 중'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('install-service-button')));
    await tester.pumpAndSettle();

    expect(calls.length, 2);
    expect(calls[1], ['kfind_service', 'install']);
    expect(find.text('지금 상태: 관리자 서비스'), findsOneWidget);
    expect(find.byKey(const ValueKey('search-service-error')), findsNothing);
  });

  testWidgets('관리자 서비스 설치 실패 시 이유 표시와 사용자 모드로 계속 쓴다는 안내가 나온다', (tester) async {
    Future<Object?> fakeApi(String method, List<Object?> args) async {
      if (args.isNotEmpty && args[0] == 'status') {
        return {
          'ok': true,
          'status': 'user',
          'status_text': '사용자 모드 실행 중',
        };
      }
      if (args.isNotEmpty && args[0] == 'install') {
        return {
          'ok': false,
          'message': '권한이 거부되었습니다(오류 5).',
          'status': 'user',
          'status_text': '사용자 모드 실행 중',
          'fallback_notice': '관리자 서비스 설치에 실패하여 사용자 모드로 계속 사용합니다.',
        };
      }
      return {'ok': false};
    }

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SearchServiceCard(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    await tester.tap(find.byKey(const ValueKey('install-service-button')));
    await tester.pumpAndSettle();

    expect(find.textContaining('권한이 거부되었습니다'), findsOneWidget);
    expect(
      find.text('관리자 서비스 설치에 실패하여 사용자 모드로 계속 사용합니다.'),
      findsOneWidget,
    );
    expect(find.text('지금 상태: 사용자 모드 실행 중'), findsOneWidget);
  });

  testWidgets('관리자 서비스 제거 클릭 시 uninstall 호출되고 상태가 갱신된다', (tester) async {
    final calls = <List<Object?>>[];
    Future<Object?> fakeApi(String method, List<Object?> args) async {
      calls.add([method, ...args]);
      if (args.isNotEmpty && args[0] == 'status') {
        return {
          'ok': true,
          'status': 'service',
          'status_text': '관리자 서비스',
        };
      }
      if (args.isNotEmpty && args[0] == 'uninstall') {
        return {
          'ok': true,
          'message': '제거 완료',
          'status': 'user',
          'status_text': '사용자 모드 실행 중',
        };
      }
      return {'ok': false};
    }

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SearchServiceCard(api: fakeApi),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.text('지금 상태: 관리자 서비스'), findsOneWidget);

    await tester.tap(find.byKey(const ValueKey('uninstall-service-button')));
    await tester.pumpAndSettle();

    expect(calls.length, 2);
    expect(calls[1], ['kfind_service', 'uninstall']);
    expect(find.text('지금 상태: 사용자 모드 실행 중'), findsOneWidget);
  });

  testWidgets('locked가 true일 때 설치·제거 버튼이 비활성화된다', (tester) async {
    Future<Object?> fakeApi(String method, List<Object?> args) async => {
          'ok': true,
          'status': 'stopped',
          'status_text': '꺼짐',
        };

    await tester.pumpWidget(
      const MaterialApp(
        home: Scaffold(
          body: SearchServiceCard(api: fakeApi, locked: true),
        ),
      ),
    );
    await tester.pumpAndSettle();

    final installBtn = tester.widget<FilledButton>(
      find.byKey(const ValueKey('install-service-button')),
    );
    expect(installBtn.onPressed, isNull);

    final uninstallBtn = tester.widget<OutlinedButton>(
      find.byKey(const ValueKey('uninstall-service-button')),
    );
    expect(uninstallBtn.onPressed, isNull);
  });
}
