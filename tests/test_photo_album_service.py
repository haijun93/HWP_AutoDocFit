"""사진뷰(PhotoAlbumService) 단위 시험 및 브리지 연동 검증 (MPLB1)."""
from __future__ import annotations

import hashlib
import io
import os
import tempfile
import unittest
from datetime import datetime
from pathlib import Path
from unittest.mock import MagicMock, patch

from defusedxml import ElementTree as ET
from PIL import Image

from docfit_app.application_service import ApplicationService
from docfit_app.photo_album_service import (
    PHOTO_EXTENSIONS,
    PhotoAlbumDb,
    PhotoAlbumService,
    read_exif_metadata,
)
from docfit_core.hwpx import open_hwpx_safely


def make_test_photo(
    path: Path | str,
    *,
    width: int = 200,
    height: int = 100,
    orientation: int | None = None,
    date_taken: str | None = None,
    color: tuple[int, int, int] = (255, 0, 0),
    fmt: str = "JPEG",
    make: str | None = None,
    model: str | None = None,
    gps: tuple[float, float] | None = None,
) -> Path:
    """단위 시험용 사진 파일을 생성한다."""
    path_obj = Path(path)
    path_obj.parent.mkdir(parents=True, exist_ok=True)
    img = Image.new("RGB", (width, height), color=color)

    if fmt.upper() == "PNG":
        img.save(path_obj, format="PNG")
        return path_obj

    exif = img.getexif()

    if orientation is not None:
        exif[0x0112] = orientation  # Orientation

    if date_taken is not None:
        # EXIF DateTimeOriginal (0x9003 in IFD 0x8769)
        ifd = exif.get_ifd(0x8769)
        ifd[0x9003] = date_taken

    if make is not None:
        exif[271] = make

    if model is not None:
        exif[272] = model

    if gps is not None:
        lat, lon = gps
        lat_ref = "N" if lat >= 0 else "S"
        lon_ref = "E" if lon >= 0 else "W"
        lat_abs = abs(lat)
        lon_abs = abs(lon)
        lat_d = int(lat_abs)
        lat_m = int((lat_abs - lat_d) * 60)
        lat_s = round(((lat_abs - lat_d) * 60 - lat_m) * 60, 4)

        lon_d = int(lon_abs)
        lon_m = int((lon_abs - lon_d) * 60)
        lon_s = round(((lon_abs - lon_d) * 60 - lon_m) * 60, 4)

        gps_ifd = exif.get_ifd(0x8825)
        gps_ifd[1] = lat_ref
        gps_ifd[2] = (lat_d, lat_m, lat_s)
        gps_ifd[3] = lon_ref
        gps_ifd[4] = (lon_d, lon_m, lon_s)

    img.save(path_obj, format="JPEG", exif=exif)
    return path_obj


def compute_sha256(path: Path | str) -> str:
    """파일의 SHA-256 해시를 계산한다."""
    h = hashlib.sha256()
    with open(path, "rb") as f:
        while chunk := f.read(65536):
            h.update(chunk)
    return h.hexdigest()


class TestPhotoAlbumService(unittest.TestCase):
    """PhotoAlbumService의 핵심 기능 단위 시험."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.db_path = self.root / "test_photos.db"
        self.thumb_dir = self.root / "thumbs"
        self.photos_dir = self.root / "photos"
        self.photos_dir.mkdir(parents=True, exist_ok=True)

        self.service = PhotoAlbumService(
            db_path=self.db_path,
            thumb_dir=self.thumb_dir,
            max_slots=2,
        )

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_exif_orientation_6_and_8_rotation(self):
        """Orientation=6 및 8 세로 사진이 정방향(가로<세로)으로 회전되어 썸네일로 생성되는지 검증."""
        # 1. Orientation=6: 원본 가로 200, 세로 100 (스마트폰 세로 촬영의 표준 EXIF)
        p6 = self.photos_dir / "photo_orient_6.jpg"
        make_test_photo(p6, width=200, height=100, orientation=6)

        # 2. Orientation=8: 원본 가로 200, 세로 100
        p8 = self.photos_dir / "photo_orient_8.jpg"
        make_test_photo(p8, width=200, height=100, orientation=8)

        # 3. Orientation=1: 원본 가로 200, 세로 100 (회전 없음)
        p1 = self.photos_dir / "photo_orient_1.jpg"
        make_test_photo(p1, width=200, height=100, orientation=1)

        # 썸네일 생성 및 크기 검증
        res6 = self.service.get_thumbnail(p6, as_base64=False)
        self.assertTrue(res6["ok"])
        with Image.open(io.BytesIO(res6["data"])) as t6:
            # Orientation 6은 정방향 보정 시 세로 사진이 됨 (width < height)
            self.assertLess(t6.width, t6.height, f"Orientation 6 회전 실패: {t6.size}")

        res8 = self.service.get_thumbnail(p8, as_base64=False)
        self.assertTrue(res8["ok"])
        with Image.open(io.BytesIO(res8["data"])) as t8:
            # Orientation 8도 정방향 보정 시 세로 사진이 됨 (width < height)
            self.assertLess(t8.width, t8.height, f"Orientation 8 회전 실패: {t8.size}")

        res1 = self.service.get_thumbnail(p1, as_base64=False)
        self.assertTrue(res1["ok"])
        with Image.open(io.BytesIO(res1["data"])) as t1:
            # Orientation 1은 원래대로 가로 사진 (width > height)
            self.assertGreater(t1.width, t1.height, f"Orientation 1 크기 이상: {t1.size}")

    def test_year_month_grouping(self):
        """사진들의 촬영일을 기준으로 연·월 묶음 격자 목록으로 올바르게 분류·정렬되는지 검증."""
        p_nov26 = self.photos_dir / "pic_2026_11.jpg"
        make_test_photo(p_nov26, date_taken="2026:11:16 14:30:00")

        p_may26 = self.photos_dir / "pic_2026_05.jpg"
        make_test_photo(p_may26, date_taken="2026:05:01 09:15:00")

        p_mar25 = self.photos_dir / "pic_2025_03.jpg"
        make_test_photo(p_mar25, date_taken="2025:03:20 18:00:00")

        # 스캔 실행
        scan_res = self.service.scan_folder(self.photos_dir)
        self.assertTrue(scan_res["ok"])
        self.assertEqual(scan_res["scanned"], 3)
        self.assertEqual(scan_res["added"], 3)

        # 연·월 묶음 조회
        groups = self.service.list_by_date(self.photos_dir)
        self.assertEqual(len(groups), 3)

        # 최신 연·월 순으로 정렬 검증
        self.assertEqual((groups[0]["year"], groups[0]["month"]), (2026, 11))
        self.assertEqual(groups[0]["items"][0]["name"], "pic_2026_11.jpg")

        self.assertEqual((groups[1]["year"], groups[1]["month"]), (2026, 5))
        self.assertEqual(groups[1]["items"][0]["name"], "pic_2026_05.jpg")

        self.assertEqual((groups[2]["year"], groups[2]["month"]), (2025, 3))
        self.assertEqual(groups[2]["items"][0]["name"], "pic_2025_03.jpg")

    def test_incremental_scan(self):
        """증분 스캔 시 변경되지 않은 파일은 재분석을 건너뛰는지 검증."""
        p1 = self.photos_dir / "photo1.jpg"
        make_test_photo(p1, date_taken="2026:10:01 10:00:00")

        # 1차 스캔: 1개 추가
        res1 = self.service.scan_folder(self.photos_dir)
        self.assertEqual(res1["added"], 1)
        self.assertEqual(res1["updated"], 0)

        # 2차 스캔 (변화 없음): 추가 0, 갱신 0
        res2 = self.service.scan_folder(self.photos_dir)
        self.assertEqual(res2["added"], 0)
        self.assertEqual(res2["updated"], 0)

        # 3차: 새 파일 추가
        p2 = self.photos_dir / "photo2.jpg"
        make_test_photo(p2, date_taken="2026:10:02 11:00:00")
        res3 = self.service.scan_folder(self.photos_dir)
        self.assertEqual(res3["added"], 1)
        self.assertEqual(res3["updated"], 0)

    def test_favorite_and_tagging_and_search(self):
        """즐겨찾기 토글, 태그 추가/삭제, 다양한 조건의 검색이 정상 동작하는지 검증."""
        p_cat = self.photos_dir / "cat_pic.jpg"
        make_test_photo(p_cat, date_taken="2026:08:15 12:00:00")

        p_dog = self.photos_dir / "dog_pic.jpg"
        make_test_photo(p_dog, date_taken="2026:09:20 15:30:00")

        self.service.scan_folder(self.photos_dir)

        # 1. 즐겨찾기 토글
        fav1 = self.service.toggle_favorite(p_cat)
        self.assertTrue(fav1["favorite"])

        # DB 재조회 확인
        cat_info = self.service.db.get_photo(str(p_cat))
        self.assertTrue(cat_info["favorite"])

        # 즐겨찾기 해제
        fav2 = self.service.toggle_favorite(p_cat)
        self.assertFalse(fav2["favorite"])

        # 다시 켜기
        self.service.toggle_favorite(p_cat)

        # 2. 태그 추가/삭제
        self.service.add_tag(p_cat, "동물")
        self.service.add_tag(p_cat, "반려묘")
        self.service.add_tag(p_dog, "동물")

        tags_cat = self.service.get_tags(p_cat)["tags"]
        self.assertEqual(tags_cat, ["동물", "반려묘"])

        self.service.remove_tag(p_cat, "반려묘")
        tags_cat_after = self.service.get_tags(p_cat)["tags"]
        self.assertEqual(tags_cat_after, ["동물"])

        # 3. 검색
        # (a) 파일명 검색
        res_cat = self.service.search(query="cat")
        self.assertEqual(len(res_cat), 1)
        self.assertEqual(res_cat[0]["name"], "cat_pic.jpg")

        # (b) 태그 검색
        res_animals = self.service.search(tag="동물")
        self.assertEqual(len(res_animals), 2)

        # (c) 즐겨찾기 필터
        res_fav = self.service.search(favorite_only=True)
        self.assertEqual(len(res_fav), 1)
        self.assertEqual(res_fav[0]["name"], "cat_pic.jpg")

        # (d) 날짜 범위 검색
        res_date = self.service.search(start_date="2026-09-01", end_date="2026-09-30")
        self.assertEqual(len(res_date), 1)
        self.assertEqual(res_date[0]["name"], "dog_pic.jpg")

    def test_cloud_only_file_protection(self):
        """클라우드 전용 파일은 내용을 절대 열거나 읽지 않고 안전하게 처리하는지 검증."""
        cloud_file = self.photos_dir / "cloud_only.jpg"
        make_test_photo(cloud_file, width=100, height=100)

        # cloud_only 함수를 모킹하여 해당 파일에 대해 True를 반환하도록 함
        orig_cloud_only = None
        with patch("docfit_app.photo_album_service.cloud_only", return_value=True):
            # 스캔 실행
            res = self.service.scan_folder(self.photos_dir)
            self.assertTrue(res["ok"])

            # DB에 cloud: 1로 등록되었는지 확인
            photo = self.service.db.get_photo(str(cloud_file))
            self.assertIsNotNone(photo)
            self.assertTrue(photo["cloud"])

            # 썸네일 요청 시 내용 읽지 않고 CLOUD_ONLY 코드 반환 확인
            thumb_res = self.service.get_thumbnail(cloud_file)
            self.assertFalse(thumb_res["ok"])
            self.assertEqual(thumb_res["code"], "CLOUD_ONLY")

    def test_original_files_immutability(self):
        """모든 작업(스캔, 썸네일, 태그, 즐겨찾기, 일괄 사본, 사진대지 HWPX) 전후에 원본 파일의 해시가 불변인지 검증."""
        p1 = self.photos_dir / "orig1.jpg"
        make_test_photo(p1, width=300, height=200, orientation=6, date_taken="2026:11:10 10:00:00")

        p2 = self.photos_dir / "orig2.jpg"
        make_test_photo(p2, width=400, height=300, orientation=1, date_taken="2026:11:11 11:00:00")

        hash1_before = compute_sha256(p1)
        hash2_before = compute_sha256(p2)

        # 1. 스캔
        self.service.scan_folder(self.photos_dir)

        # 2. 썸네일 생성
        self.service.get_thumbnail(p1)
        self.service.get_thumbnail(p2)

        # 3. 즐겨찾기 및 태그
        self.service.toggle_favorite(p1)
        self.service.add_tag(p1, "테스트")
        self.service.add_tag(p2, "풍경")

        # 4. 공문 첨부용 일괄 사본 저장
        export_dir = self.root / "exported"
        self.service.batch_export([p1, p2], export_dir, max_width=150)

        # 5. 사진대지 HWPX 생성
        hwpx_path = self.root / "photo_sheet.hwpx"
        self.service.build_photo_sheet_hwpx(
            [
                {"path": str(p1), "caption": "사진 1번 설명"},
                {"path": str(p2), "caption": "사진 2번 설명"},
            ],
            hwpx_path,
            layout="2x2",
        )

        # 6. 유사·중복 사진 검사
        self.service.command("find_duplicates", {"folder": str(self.photos_dir), "threshold": 6})

        # 7. 비파괴 사본 편집
        self.service.command(
            "edit_copy",
            {
                "path": str(p1),
                "rotate_deg": 90,
                "crop": [10, 10, 100, 100],
                "brightness": 1.1,
                "contrast": 1.1,
            },
        )

        # 8. 오프라인 그룹화
        self.service.command("place_groups", {"folder": str(self.photos_dir), "group_by": "folder"})

        # 해시 재계산 및 불변 검증
        hash1_after = compute_sha256(p1)
        hash2_after = compute_sha256(p2)

        self.assertEqual(hash1_before, hash1_after, "원본 사진 1번이 수정되었습니다!")
        self.assertEqual(hash2_before, hash2_after, "원본 사진 2번이 수정되었습니다!")

    def test_batch_export_reduction_and_orientation(self):
        """공문 첨부용 일괄 처리: max_width 축소 및 EXIF 회전 정방향 보정 사본 저장 검증."""
        p_big_rot6 = self.photos_dir / "big_rot6.jpg"
        make_test_photo(p_big_rot6, width=2000, height=1000, orientation=6)

        export_dir = self.root / "batch_out"
        res = self.service.batch_export(
            [p_big_rot6],
            export_dir,
            max_width=800,
            quality=80,
            suffix="_shrunk",
            naming="suffix",
        )
        self.assertTrue(res["ok"])
        out_item = res["results"][0]
        self.assertTrue(out_item["ok"])
        out_path = Path(out_item["output_path"])
        self.assertTrue(out_path.is_file())

        with Image.open(out_path) as out_img:
            # Orientation 6 정방향 변환으로 세로 사진이 됨: 원본 가로 2000 x 1000 -> 세로 1000 x 2000
            # max_width=800이므로 축소 후 세로 비율 맞춤
            self.assertLessEqual(out_img.width, 800)
            self.assertLess(out_img.width, out_img.height, "일괄 사본에서 세로 회전 정방향이 반영되지 않았습니다.")

    def test_photo_sheet_hwpx_generation(self):
        """2×2 및 2×3 사진대지 HWPX 생성 및 defusedxml 검증."""
        photos = []
        for i in range(4):
            p = self.photos_dir / f"sheet_{i}.jpg"
            make_test_photo(p, width=300, height=200, date_taken=f"2026:11:1{i} 10:00:00")
            photos.append({"path": str(p), "caption": f"현장 사진 {i + 1}호", "date": f"2026-11-1{i} 10:00:00"})

        out_hwpx_2x2 = self.root / "sheet_2x2.hwpx"
        res = self.service.build_photo_sheet_hwpx(photos, out_hwpx_2x2, layout="2x2", title="현장 사진대지 2x2")
        self.assertTrue(res["ok"])
        self.assertTrue(out_hwpx_2x2.is_file())

        # HWPX 무결성 및 XML 구조 검증 (defusedxml)
        with open_hwpx_safely(out_hwpx_2x2) as z:
            names = z.namelist()
            self.assertIn("mimetype", names)
            self.assertIn("Contents/section0.xml", names)
            self.assertIn("Contents/header.xml", names)
            self.assertIn("Contents/content.hpf", names)

            # BinData에 4장의 사진이 들어갔는지 확인
            for i in range(4):
                self.assertIn(f"BinData/photo_{i + 1}.jpg", names)

            # section0.xml 파싱 및 캡션 텍스트 확인
            root = ET.fromstring(z.read("Contents/section0.xml"))
            all_text = "".join(root.itertext())
            self.assertIn("현장 사진대지 2x2", all_text)
            self.assertIn("현장 사진 1호", all_text)
            self.assertIn("현장 사진 4호", all_text)

        # 6장 2×3 레이아웃 사진대지 시험
        photos_6 = list(photos)
        for i in (4, 5):
            p = self.photos_dir / f"sheet_{i}.jpg"
            make_test_photo(p, width=300, height=200)
            photos_6.append({"path": str(p), "caption": f"추가 사진 {i + 1}호"})

        out_hwpx_2x3 = self.root / "sheet_2x3.hwpx"
        res_2x3 = self.service.build_photo_sheet_hwpx(photos_6, out_hwpx_2x3, layout="2x3", title="현장 사진대지 2x3")
        self.assertTrue(res_2x3["ok"])
        self.assertTrue(out_hwpx_2x3.is_file())

    def test_find_duplicates_dhash_groups_similar_images(self):
        """동일 사진 2장 + 유사 사본 1장 + 다른 패턴 1장 + cloud-only 1장을 스캔 후 find_duplicates 검증."""
        dup_dir = self.photos_dir / "dups"
        dup_dir.mkdir(parents=True, exist_ok=True)

        # 1. 원본 패턴 이미지
        p1 = dup_dir / "pattern_base.jpg"
        im1 = Image.new("RGB", (100, 80))
        for y in range(80):
            for x in range(100):
                im1.putpixel((x, y), (((x * 3) ^ (y * 5)) % 256, (x * 2) % 256, (y * 4) % 256))
        im1.save(p1, format="JPEG")

        # 2. 동일한 이미지 (바이트 복사본)
        p2 = dup_dir / "pattern_exact_copy.jpg"
        p2.write_bytes(p1.read_bytes())

        # 3. 밝기만 살짝 다른 유사 사본
        p3 = dup_dir / "pattern_slightly_brighter.jpg"
        im3 = Image.new("RGB", (100, 80))
        for y in range(80):
            for x in range(100):
                im3.putpixel(
                    (x, y),
                    (
                        min(255, ((x * 3) ^ (y * 5)) % 256 + 10),
                        min(255, (x * 2) % 256 + 10),
                        min(255, (y * 4) % 256 + 10),
                    ),
                )
        im3.save(p3, format="JPEG")

        # 4. 완전히 다른 패턴 이미지 (체커보드)
        p4 = dup_dir / "pattern_checkerboard_different.jpg"
        im4 = Image.new("RGB", (100, 80))
        for y in range(80):
            for x in range(100):
                val = 255 if ((x // 10) + (y // 10)) % 2 == 0 else 0
                im4.putpixel((x, y), (val, val, val))
        im4.save(p4, format="JPEG")

        # 5. 클라우드 전용 파일
        p_cloud = dup_dir / "pattern_cloud.jpg"
        p_cloud.write_bytes(p1.read_bytes())

        # 스캔 수행
        scan_res = self.service.scan_folder(dup_dir)
        self.assertTrue(scan_res["ok"])
        self.assertEqual(scan_res["scanned"], 5)

        # p_cloud를 DB에서 cloud=1로 설정
        self.service.db.upsert_photo(
            path=str(p_cloud.resolve()),
            folder=str(dup_dir.resolve()),
            filename=p_cloud.name,
            size=p_cloud.stat().st_size,
            mtime=p_cloud.stat().st_mtime,
            date_taken="2026-11-17 00:00:00",
            orientation=1,
            cloud=1,
        )

        # find_duplicates 실행
        res = self.service.command("find_duplicates", {"folder": str(dup_dir), "threshold": 6})
        self.assertTrue(res["ok"])
        self.assertEqual(res["threshold"], 6)
        self.assertEqual(res["skipped_cloud"], 1)

        # 그룹 검증: 유사 3장 (p1, p2, p3)이 1개 그룹으로 묶이고 체커보드는 제외되어야 함
        groups = res["groups"]
        self.assertEqual(len(groups), 1, f"그룹 개수 불일치: {groups}")
        group = groups[0]
        self.assertEqual(group["count"], 3)

        group_paths = {item["path"] for item in group["items"]}
        self.assertIn(str(p1.resolve()), group_paths)
        self.assertIn(str(p2.resolve()), group_paths)
        self.assertIn(str(p3.resolve()), group_paths)
        self.assertNotIn(str(p4.resolve()), group_paths)
        self.assertNotIn(str(p_cloud.resolve()), group_paths)

        # 대표와의 거리가 0 이상 6 이하인지 검증
        for item in group["items"]:
            self.assertLessEqual(item["distance"], 6)
            self.assertEqual(len(item["dhash"]), 16)

    def test_edit_copy_non_destructive_and_preserves_original_sha256(self):
        """원본 SHA-256 불변, 회전/자르기/밝기/대비 편집 사본 생성 및 덮어쓰기·클라우드 차단 검증."""
        edit_dir = self.photos_dir / "edit_test"
        edit_dir.mkdir(parents=True, exist_ok=True)

        orig_path = edit_dir / "original_sample.jpg"
        make_test_photo(orig_path, width=400, height=300, color=(120, 80, 200))
        orig_sha = compute_sha256(orig_path)

        out_path = edit_dir / "edited_sample.jpg"

        # 1. 복합 편집 실행 (crop, rotate_deg=90, brightness=1.2, contrast=1.1)
        res = self.service.command(
            "edit_copy",
            {
                "path": str(orig_path),
                "output_path": str(out_path),
                "crop": [50, 40, 350, 260],
                "rotate_deg": 90,
                "brightness": 1.2,
                "contrast": 1.1,
            },
        )
        self.assertTrue(res["ok"], f"edit_copy 실패: {res}")
        self.assertEqual(res["source"], str(orig_path.resolve()))
        self.assertEqual(res["output_path"], str(out_path.resolve()))
        self.assertTrue(out_path.is_file())

        # 원본 파일 무결성(SHA-256) 검증: 100% 동일해야 함
        self.assertEqual(compute_sha256(orig_path), orig_sha, "원본 파일이 수정되었습니다!")

        # 사본 크기 및 변형 검증
        # 원본 (400, 300) -> 90도 회전 시 (300, 400) -> crop [50, 40, 350(clamp 300), 260] -> 너비 250, 높이 220
        with Image.open(out_path) as out_img:
            self.assertEqual(out_img.size, (res["width"], res["height"]))
            self.assertEqual(out_img.width, 250)
            self.assertEqual(out_img.height, 220)

        # 2. output_path == source_path 시 OVERWRITE_FORBIDDEN 거부 검증
        res_overwrite = self.service.command(
            "edit_copy",
            {
                "path": str(orig_path),
                "output_path": str(orig_path),
                "rotate_deg": 180,
            },
        )
        self.assertFalse(res_overwrite["ok"])
        self.assertEqual(res_overwrite["code"], "OVERWRITE_FORBIDDEN")
        self.assertEqual(compute_sha256(orig_path), orig_sha)

        # 3. cloud 파일 편집 시도 시 CLOUD_ONLY 거부 검증
        cloud_path = edit_dir / "cloud_sample.jpg"
        cloud_path.write_bytes(orig_path.read_bytes())
        self.service.db.upsert_photo(
            path=str(cloud_path.resolve()),
            folder=str(edit_dir.resolve()),
            filename=cloud_path.name,
            size=cloud_path.stat().st_size,
            mtime=cloud_path.stat().st_mtime,
            date_taken="2026-11-17 00:00:00",
            orientation=1,
            cloud=1,
        )
        res_cloud = self.service.command(
            "edit_copy",
            {
                "path": str(cloud_path),
                "rotate_deg": 90,
            },
        )
        self.assertFalse(res_cloud["ok"])
        self.assertEqual(res_cloud["code"], "CLOUD_ONLY")

    def test_place_groups_offline_by_folder_camera_and_location(self):
        """2개 하위 폴더 + 카메라 Make/Model + GPS 좌표 및 PNG 혼합 시 오프라인 그룹화 검증."""
        sub_a = self.photos_dir / "sub_folder_a"
        sub_b = self.photos_dir / "sub_folder_b"
        sub_a.mkdir(parents=True, exist_ok=True)
        sub_b.mkdir(parents=True, exist_ok=True)

        # 사진 1: sub_a, Sony A7M4, 서울 시청 GPS (37.5665, 126.9780)
        p_a1 = sub_a / "photo_a1.jpg"
        make_test_photo(
            p_a1,
            make="Sony",
            model="A7M4",
            gps=(37.5665, 126.9780),
            date_taken="2026:11:15 10:00:00",
        )

        # 사진 2: sub_a, PNG 형식 (EXIF/GPS 없음)
        p_a2 = sub_a / "photo_a2.png"
        make_test_photo(p_a2, fmt="PNG")

        # 사진 3: sub_b, Apple iPhone 15, 부산 시청 GPS (35.1796, 129.0756)
        p_b1 = sub_b / "photo_b1.jpg"
        make_test_photo(
            p_b1,
            make="Apple",
            model="iPhone 15",
            gps=(35.1796, 129.0756),
            date_taken="2026:11:16 11:00:00",
        )

        # 재귀 스캔 수행
        scan_res = self.service.scan_folder(self.photos_dir, recursive=True)
        self.assertTrue(scan_res["ok"])
        self.assertEqual(scan_res["scanned"], 3)

        # 1. group_by="folder"
        res_folder = self.service.command("place_groups", {"folder": str(self.photos_dir), "group_by": "folder"})
        self.assertTrue(res_folder["ok"])
        self.assertEqual(res_folder["total_photos"], 3)
        folder_labels = [g["label"] for g in res_folder["groups"]]
        self.assertIn("sub_folder_a", folder_labels)
        self.assertIn("sub_folder_b", folder_labels)

        # 2. group_by="camera"
        res_camera = self.service.command("place_groups", {"folder": str(self.photos_dir), "group_by": "camera"})
        self.assertTrue(res_camera["ok"])
        camera_labels = [g["label"] for g in res_camera["groups"]]
        self.assertIn("Sony A7M4", camera_labels)
        self.assertIn("Apple iPhone 15", camera_labels)
        self.assertIn("카메라 정보 없음", camera_labels)

        # 3. group_by="location"
        res_loc = self.service.command("place_groups", {"folder": str(self.photos_dir), "group_by": "location"})
        self.assertTrue(res_loc["ok"])
        loc_labels = [g["label"] for g in res_loc["groups"]]
        self.assertIn("GPS (37.57, 126.98)", loc_labels)
        self.assertIn("GPS (35.18, 129.08)", loc_labels)
        self.assertIn("sub_folder_a", loc_labels)  # GPS 없는 파일은 상위 폴더명 fallback

        # 브랜드 명칭 일관성 검증: 금지어가 일절 없는지 확인
        raw_json_str = str(res_loc)
        forbidden_words = ["사" + "진첩", "몽" + "땅" + "사" + "진첩"]
        for forbidden in forbidden_words:
            self.assertNotIn(forbidden, raw_json_str)


class TestPhotoAlbumBridge(unittest.TestCase):
    """desktop_web_ui.py 브리지 및 ApplicationService 연동 허용 목록 시험."""

    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.root = Path(self.temp_dir.name)
        self.settings_file = self.root / "settings.json"
        self.settings_file.write_text("{}", encoding="utf-8")
        self.app = ApplicationService(self.settings_file)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_browser_api_allowlist_contains_photo_album(self):
        """desktop_web_ui._BrowserApi에 photo_album이 등록되어 있는지 검증."""
        from desktop_web_ui import DesktopWebBridge, _BrowserApi

        bridge = DesktopWebBridge(MagicMock())
        api = _BrowserApi(bridge)
        self.assertTrue(hasattr(api, "photo_album"))
        self.assertTrue(callable(getattr(api, "photo_album")))

    def test_desktop_bridge_photo_album_dispatch(self):
        """DesktopWebBridge.photo_album 명령이 정상 디스패치되는지 검증."""
        from desktop_web_ui import DesktopWebBridge

        bridge = DesktopWebBridge(MagicMock())
        res = bridge.photo_album("list_folders", {})
        self.assertTrue(res.get("ok"))
        self.assertIsInstance(res.get("folders"), list)

    def test_application_service_dispatch_photo_album(self):
        """ApplicationService.dispatch에서 photo_album 명령이 정상 처리되는지 검증."""
        res = self.app.dispatch("photo_album", ["list_folders", {}])
        self.assertTrue(res.get("ok"))
        self.assertIsInstance(res.get("folders"), list)

        # 지원하지 않는 명령에 대한 에러 처리
        res_invalid = self.app.dispatch("photo_album", ["invalid_command_xyz", {}])
        self.assertFalse(res_invalid.get("ok"))
        self.assertEqual(res_invalid.get("code"), "NOT_AVAILABLE")


if __name__ == "__main__":
    unittest.main()
