/* Gribble stands in front of the sample's standing stones on Holtburg's west side, facing south.
   One instance, so no other instance of the landblock is touched. */
DELETE FROM `landblock_instance` WHERE `guid` = {{guid:guide@A9B4}};

INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, `is_Link_Child`, `last_Modified`)
VALUES ({{guid:guide@A9B4}}, {{wcid:guide}}, 0xA9B40009, 36, 18, 85.505, 0, 0, 0, -1, False, '2026-10-18 00:00:00') /* Gribble the Painted */
/* @teleloc 0xA9B40009 [36.000000 18.000000 85.505000] 0.000000 0.000000 0.000000 -1.000000 */;
