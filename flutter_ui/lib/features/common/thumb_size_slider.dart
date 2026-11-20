import 'package:flutter/material.dart';

import '../../core/api/abbreviation_api.dart' show ApiCall;

/// 문서뷰·사진뷰가 공유하는 썸네일 크기 범위.
const double kThumbSizeMin = 100;
const double kThumbSizeMax = 240;
const int kThumbSizeDivisions = 7;

/// 화면별 썸네일 크기를 설정에서 불러온다(실패하면 null).
Future<double?> loadThumbSize(ApiCall api, String service) async {
  try {
    final res = await api(service, ['thumb_size_get', {}]);
    if (res is Map && res['ok'] == true && res['size'] is num) {
      return (res['size'] as num).toDouble().clamp(kThumbSizeMin, kThumbSizeMax);
    }
  } catch (_) {}
  return null;
}

/// 화면별 썸네일 크기를 설정에 저장한다(실패해도 화면 동작에는 영향 없음).
Future<void> saveThumbSize(ApiCall api, String service, double size) async {
  try {
    await api(service, [
      'thumb_size_set',
      {'size': size},
    ]);
  } catch (_) {}
}

/// 썸네일 크기 조절 슬라이더(아이콘 + 80px 슬라이더).
class ThumbSizeSlider extends StatelessWidget {
  const ThumbSizeSlider({
    super.key,
    required this.sliderKey,
    required this.value,
    required this.onChanged,
    this.onChangeEnd,
    this.icon = Icons.photo_size_select_actual_outlined,
  });

  final Key sliderKey;
  final double value;
  final ValueChanged<double> onChanged;
  final ValueChanged<double>? onChangeEnd;
  final IconData icon;

  @override
  Widget build(BuildContext context) {
    return Tooltip(
      message: '썸네일 크기 조절',
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(icon, size: 16, color: Colors.grey),
          SizedBox(
            width: 80,
            child: Slider(
              key: sliderKey,
              value: value.clamp(kThumbSizeMin, kThumbSizeMax),
              min: kThumbSizeMin,
              max: kThumbSizeMax,
              divisions: kThumbSizeDivisions,
              onChanged: onChanged,
              onChangeEnd: onChangeEnd,
            ),
          ),
        ],
      ),
    );
  }
}
