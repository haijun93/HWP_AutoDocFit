import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart';
import '../../core/theme/app_theme.dart';
import 'kiosk_theme.dart';

/// 검색 서비스(관리자 kfind 서비스) 설치·제거 및 상태 표시 카드.
class SearchServiceCard extends StatefulWidget {
  const SearchServiceCard({
    super.key,
    required this.api,
    this.locked = false,
  });

  final ApiCall api;
  final bool locked;

  @override
  State<SearchServiceCard> createState() => _SearchServiceCardState();
}

class _SearchServiceCardState extends State<SearchServiceCard> {
  bool _busy = false;
  String _status = 'stopped';
  String _statusText = '꺼짐';
  String? _errorMessage;
  bool _showFallbackNotice = false;

  @override
  void initState() {
    super.initState();
    _loadStatus();
  }

  Future<void> _loadStatus() async {
    setState(() => _busy = true);
    try {
      final response = await widget.api('kfind_service', ['status']);
      if (response is Map && response['ok'] == true) {
        if (!mounted) return;
        setState(() {
          _status = '${response['status'] ?? 'stopped'}';
          _statusText = '${response['status_text'] ?? '꺼짐'}';
          _errorMessage = null;
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _errorMessage = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _install() async {
    setState(() {
      _busy = true;
      _errorMessage = null;
      _showFallbackNotice = false;
    });
    try {
      final response = await widget.api('kfind_service', ['install']);
      if (!mounted) return;
      if (response is Map && response['ok'] == true) {
        setState(() {
          _status = '${response['status'] ?? 'service'}';
          _statusText = '${response['status_text'] ?? '관리자 서비스'}';
          _errorMessage = null;
          _showFallbackNotice = false;
        });
      } else {
        final msg = response is Map ? response['message'] : null;
        setState(() {
          _errorMessage = msg != null ? '$msg' : '관리자 서비스 설치에 실패했습니다.';
          _showFallbackNotice = true;
          _status = response is Map && response['status'] != null
              ? '${response['status']}'
              : 'user';
          _statusText = response is Map && response['status_text'] != null
              ? '${response['status_text']}'
              : '사용자 모드 실행 중';
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() {
        _errorMessage = '$e';
        _showFallbackNotice = true;
        _status = 'user';
        _statusText = '사용자 모드 실행 중';
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _uninstall() async {
    setState(() {
      _busy = true;
      _errorMessage = null;
      _showFallbackNotice = false;
    });
    try {
      final response = await widget.api('kfind_service', ['uninstall']);
      if (!mounted) return;
      if (response is Map && response['ok'] == true) {
        setState(() {
          _status = '${response['status'] ?? 'user'}';
          _statusText = '${response['status_text'] ?? '사용자 모드 실행 중'}';
          _errorMessage = null;
          _showFallbackNotice = false;
        });
      } else {
        final msg = response is Map ? response['message'] : null;
        setState(() {
          _errorMessage = msg != null ? '$msg' : '관리자 서비스 제거에 실패했습니다.';
          if (response is Map && response['status'] != null) {
            _status = '${response['status']}';
            _statusText = '${response['status_text'] ?? _statusText}';
          }
        });
      }
    } catch (e) {
      if (!mounted) return;
      setState(() => _errorMessage = '$e');
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final k = context.kiosk;
    return Container(
      margin: const EdgeInsets.only(top: 12),
      padding: const EdgeInsets.all(16),
      decoration: BoxDecoration(
        color: k.card,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(Icons.manage_search_rounded, size: 20, color: k.blue),
              const SizedBox(width: 8),
              Text('검색 서비스', style: Kiosk.cardTitle(context)),
              if (_busy) ...[
                const SizedBox(width: 10),
                const SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ],
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '문서 본문과 파일명을 빠르게 찾는 kfind 색인 서비스를 관리합니다.',
            style: Kiosk.body(context),
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Text(
                '지금 상태: $_statusText',
                key: const ValueKey('search-service-status'),
                style: Kiosk.body(context).copyWith(
                  color: _status == 'service'
                      ? k.blue
                      : _status == 'user'
                          ? k.ink
                          : k.sub,
                  fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            '※ 관리자 서비스로 설치하면 Windows 관리자 권한 확인(UAC) 창이 뜹니다.',
            key: const ValueKey('search-service-uac-notice'),
            style: Kiosk.caption(context),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              FilledButton.icon(
                key: const ValueKey('install-service-button'),
                style: FilledButton.styleFrom(
                  backgroundColor: k.blueFill,
                  foregroundColor: k.onBlue,
                  disabledBackgroundColor: k.disabledFill,
                ),
                onPressed: widget.locked || _busy ? null : _install,
                icon: const Icon(Icons.security, size: 16),
                label: const Text('관리자 서비스로 설치'),
              ),
              OutlinedButton.icon(
                key: const ValueKey('uninstall-service-button'),
                style: OutlinedButton.styleFrom(
                  foregroundColor: k.sub,
                ),
                onPressed: widget.locked || _busy ? null : _uninstall,
                icon: const Icon(Icons.delete_outline, size: 16),
                label: const Text('관리자 서비스 제거'),
              ),
            ],
          ),
          if (_errorMessage != null) ...[
            const SizedBox(height: 10),
            Container(
              width: double.infinity,
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: k.secondaryFill,
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: k.line),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    '실패 이유: $_errorMessage',
                    key: const ValueKey('search-service-error'),
                    style: Kiosk.body(context).copyWith(
                      color: k.blue,
                      fontWeight: FontWeight.w600,
                    ),
                  ),
                  if (_showFallbackNotice) ...[
                    const SizedBox(height: 4),
                    Text(
                      '관리자 서비스 설치에 실패하여 사용자 모드로 계속 사용합니다.',
                      key: const ValueKey('search-service-fallback-notice'),
                      style: Kiosk.body(context),
                    ),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}
