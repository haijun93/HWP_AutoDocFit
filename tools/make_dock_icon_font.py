"""Flutter MaterialIcons-Regular.otf(Apache-2.0)에서 독 아이콘 코드포인트만 남긴 서브셋을 만든다.
사용: python tools/make_dock_icon_font.py <MaterialIcons-Regular.otf> <출력 .otf>
코드포인트 목록은 dock/src/lib.rs 의 ICON_CODEPOINTS 와 같아야 한다(Rust 단위 시험이 고정)."""
import sys
from fontTools import subset

CODEPOINTS = [0xefae, 0xf41d, 0xf05ed, 0xee36, 0xf0616, 0xef02, 0xe20b, 0xe316,
              0xf0d7, 0xf29d, 0xe678, 0xf2a4, 0xefa0, 0xf1c2, 0xf3c3]

def main(src, dst):
    opts = subset.Options(); opts.layout_features = []; opts.name_IDs = ["*"]; opts.notdef_outline = True
    font = subset.load_font(src, opts)
    s = subset.Subsetter(opts); s.populate(unicodes=CODEPOINTS); s.subset(font)
    subset.save_font(font, dst, opts)

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
