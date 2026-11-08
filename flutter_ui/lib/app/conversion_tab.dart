import 'package:flutter/material.dart';

import '../features/conversion/conversion_workspace.dart';

class ConversionTab extends StatefulWidget {
  const ConversionTab({super.key, required this.host});
  final ConversionHost? Function() host;
  @override
  State<ConversionTab> createState() => _ConversionTabState();
}

class _ConversionTabState extends State<ConversionTab> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) setState(() {});
    });
  }

  @override
  Widget build(BuildContext context) {
    final host = widget.host();
    if (host == null) return const Center(child: CircularProgressIndicator());
    return ConversionWorkspace(host: host);
  }
}
