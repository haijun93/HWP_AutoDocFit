import 'package:flutter/material.dart';

ThemeData buildCompactTheme() {
  const accent = Color(0xff0078d4);
  const surface = Color(0xffffffff);
  const outline = Color(0xffe5e5e5);
  const shape = RoundedRectangleBorder(
    borderRadius: BorderRadius.all(Radius.circular(3)),
  );

  final scheme = ColorScheme.fromSeed(
    seedColor: accent,
    brightness: Brightness.light,
    surface: surface,
  ).copyWith(
    primary: accent,
    onPrimary: Colors.white,
    surface: surface,
    outline: outline,
  );

  return ThemeData(
    useMaterial3: true,
    visualDensity: VisualDensity.compact,
    materialTapTargetSize: MaterialTapTargetSize.shrinkWrap,
    fontFamily: 'NotoSansKR',
    fontFamilyFallback: const ['NotoSansSymbols2', 'STIXTwoMath'],
    colorScheme: scheme,
    scaffoldBackgroundColor: const Color(0xfff3f3f3),
    textTheme: const TextTheme(
      bodyMedium: TextStyle(fontSize: 13),
      titleMedium: TextStyle(fontSize: 14),
    ),
    filledButtonTheme: FilledButtonThemeData(
      style: FilledButton.styleFrom(
        minimumSize: const Size(0, 28),
        padding: const EdgeInsets.symmetric(horizontal: 10),
        shape: shape,
      ),
    ),
    outlinedButtonTheme: OutlinedButtonThemeData(
      style: OutlinedButton.styleFrom(
        minimumSize: const Size(0, 28),
        padding: const EdgeInsets.symmetric(horizontal: 10),
        shape: shape,
      ),
    ),
    textButtonTheme: TextButtonThemeData(
      style: TextButton.styleFrom(
        minimumSize: const Size(0, 28),
        padding: const EdgeInsets.symmetric(horizontal: 10),
        shape: shape,
      ),
    ),
    iconButtonTheme: const IconButtonThemeData(
      style: ButtonStyle(
        minimumSize: WidgetStatePropertyAll(Size(28, 28)),
        fixedSize: WidgetStatePropertyAll(Size(28, 28)),
        padding: WidgetStatePropertyAll(EdgeInsets.zero),
        iconSize: WidgetStatePropertyAll(16),
        tapTargetSize: MaterialTapTargetSize.shrinkWrap,
      ),
    ),
    inputDecorationTheme: const InputDecorationTheme(
      isDense: true,
      contentPadding: EdgeInsets.symmetric(horizontal: 8, vertical: 6),
      border: OutlineInputBorder(
        borderRadius: BorderRadius.all(Radius.circular(3)),
        borderSide: BorderSide(color: outline),
      ),
      enabledBorder: OutlineInputBorder(
        borderRadius: BorderRadius.all(Radius.circular(3)),
        borderSide: BorderSide(color: outline),
      ),
      focusedBorder: OutlineInputBorder(
        borderRadius: BorderRadius.all(Radius.circular(3)),
        borderSide: BorderSide(color: accent),
      ),
    ),
    cardTheme: const CardThemeData(
      color: surface,
      elevation: 0,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.all(Radius.circular(3)),
        side: BorderSide(color: outline, width: 1),
      ),
    ),
  );
}
