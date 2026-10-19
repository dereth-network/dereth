/* Gribble, a drudge in the sample pack's stripes who greets whoever uses him. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:guide}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:guide}}, 'sample-gribble', 10, '2026-10-18 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:guide}},   1,         16) /* ItemType - Creature */
     , ({{wcid:guide}},   2,          3) /* CreatureType - Drudge */
     , ({{wcid:guide}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:guide}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:guide}},  16,         32) /* ItemUseable - Remote */
     , ({{wcid:guide}},  25,          5) /* Level */
     , ({{wcid:guide}},  93,    6292504) /* PhysicsState - ReportCollisions, IgnoreCollisions, Gravity, ReportCollisionsAsEnvironment, EdgeSlide */
     , ({{wcid:guide}},  95,          8) /* RadarBlipColor - Yellow */
     , ({{wcid:guide}}, 133,          4) /* ShowableOnRadar - ShowAlways */
     , ({{wcid:guide}}, 134,         16) /* PlayerKillerStatus - RubberGlue */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:guide}},   1, True ) /* Stuck */
     , ({{wcid:guide}},  19, False) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:guide}},  54,       3) /* UseRadius */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:guide}},   1, 'Gribble the Painted') /* Name */
     , ({{wcid:guide}},   5, 'Painted Drudge') /* Template */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:guide}},   1, 0x020007DD) /* Setup */
     , ({{wcid:guide}},   2, 0x090001D0) /* MotionTable */
     , ({{wcid:guide}},   3, 0x20000007) /* SoundTable */
     , ({{wcid:guide}},   6, 0x04000F6C) /* PaletteBase */
     , ({{wcid:guide}},   8, 0x06001035) /* Icon */
     , ({{wcid:guide}},  22, 0x3400001A) /* PhysicsEffectTable */;

INSERT INTO `weenie_properties_anim_part` (`object_Id`, `index`, `animation_Id`)
VALUES ({{wcid:guide}},  3, 0x01001B82)
     , ({{wcid:guide}},  6, 0x01001B85);

/* The pack's own texture, in place of the one those two parts wear. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:guide}}, 3, 0x050018E5, {{surface-texture:stripes}})
     , ({{wcid:guide}}, 6, 0x050018E5, {{surface-texture:stripes}});

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:guide}},   1, 100, 0, 0) /* Strength */
     , ({{wcid:guide}},   2, 100, 0, 0) /* Endurance */
     , ({{wcid:guide}},   3, 100, 0, 0) /* Quickness */
     , ({{wcid:guide}},   4, 100, 0, 0) /* Coordination */
     , ({{wcid:guide}},   5, 100, 0, 0) /* Focus */
     , ({{wcid:guide}},   6, 100, 0, 0) /* Self */;

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:guide}},   1,  50, 0, 0, 100) /* MaxHealth */
     , ({{wcid:guide}},   3,  50, 0, 0, 150) /* MaxStamina */
     , ({{wcid:guide}},   5,  50, 0, 0, 150) /* MaxMana */;

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:guide}},  7 /* Use */,      1, NULL, NULL, NULL, NULL, NULL, NULL, NULL);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`, `test_String`, `min`, `max`, `min_64`, `max_64`, `min_Dbl`, `max_Dbl`, `stat`, `display`, `amount`, `amount_64`, `hero_X_P_64`, `percent`, `spell_Id`, `wealth_Rating`, `treasure_Class`, `treasure_Type`, `p_Script`, `sound`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`, `obj_Cell_Id`, `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`)
VALUES (@parent_id,  0,  10 /* Tell */, 0, 1, NULL, 'Welcome! A content pack put me here, with the stones, the hill and these stripes.', NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL);
