import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import '../../core/api/abbreviation_api.dart';
import '../diagrams/diagram_editor_screen_native.dart';

class FlowEditorScreen extends StatelessWidget {
  const FlowEditorScreen({
    super.key,
    required this.api,
    this.initialMode = 'diagram',
    this.isModal = true,
    this.photoPasteTrigger,
  });
  final ApiCall api;
  final String initialMode;
  final bool isModal;
  final ValueListenable<int>? photoPasteTrigger;

  @override
  Widget build(BuildContext context) => DiagramEditorScreen(
        api: api,
        initialMode: initialMode,
        isModal: isModal,
        photoPasteTrigger: photoPasteTrigger,
      );
}
