import 'package:flutter/material.dart';

class DocumentArtwork extends StatelessWidget {
  const DocumentArtwork({super.key});
  @override
  Widget build(BuildContext context) => ExcludeSemantics(
    child: Stack(
      alignment: Alignment.center,
      children: [
        Container(
          width: 185,
          height: 185,
          decoration: const BoxDecoration(
            shape: BoxShape.circle,
            color: Color(0x55ffffff),
          ),
        ),
        Transform.rotate(
          angle: -.14,
          child: Container(
            width: 121,
            height: 158,
            decoration: BoxDecoration(
              color: const Color(0xffb9cdef),
              borderRadius: BorderRadius.circular(18),
            ),
          ),
        ),
        Transform.translate(
          offset: const Offset(13, -5),
          child: Transform.rotate(
            angle: .08,
            child: Container(
              width: 120,
              height: 158,
              padding: const EdgeInsets.all(19),
              decoration: BoxDecoration(
                color: Colors.white,
                borderRadius: BorderRadius.circular(18),
                boxShadow: const [
                  BoxShadow(
                    color: Color(0x152b527e),
                    blurRadius: 24,
                    offset: Offset(0, 12),
                  ),
                ],
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(
                    width: 28,
                    height: 28,
                    decoration: BoxDecoration(
                      color: const Color(0xffe7efff),
                      borderRadius: BorderRadius.circular(9),
                    ),
                    child: const Icon(
                      Icons.format_align_left_rounded,
                      color: Color(0xff2563eb),
                      size: 17,
                    ),
                  ),
                  const SizedBox(height: 20),
                  for (final w in [78.0, 65.0, 78.0, 43.0])
                    Padding(
                      padding: const EdgeInsets.only(bottom: 9),
                      child: Container(
                        width: w,
                        height: 5,
                        decoration: BoxDecoration(
                          color: const Color(0xffe8edf5),
                          borderRadius: BorderRadius.circular(3),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
        Positioned(
          right: 17,
          bottom: 18,
          child: Container(
            width: 49,
            height: 49,
            decoration: BoxDecoration(
              color: Color(0xff287c72),
              borderRadius: BorderRadius.circular(17),
              border: Border.all(color: const Color(0xffecf6f2), width: 4),
            ),
            child: const Icon(
              Icons.check_rounded,
              color: Colors.white,
              size: 28,
            ),
          ),
        ),
      ],
    ),
  );
}
