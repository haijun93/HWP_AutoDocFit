import 'package:flutter/widgets.dart';

class LifecycleRedraw extends StatefulWidget {
  const LifecycleRedraw({super.key, required this.child});
  final Widget child;

  @override
  State<LifecycleRedraw> createState() => LifecycleRedrawState();
}

class LifecycleRedrawState extends State<LifecycleRedraw>
    with WidgetsBindingObserver {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      WidgetsBinding.instance.scheduleFrame();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.child;
}
