/* Tyrant (look 2): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant2}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant2}}, 'ac2-tyrant2', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant2}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant2}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant2}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant2}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant2}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant2}},  25,          5) /* Level */
     , ({{wcid:tyrant2}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant2}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant2}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant2}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant2}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant2}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant2}},   1, True ) /* Stuck */
     , ({{wcid:tyrant2}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant2}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant2}},  13, False) /* Ethereal */
     , ({{wcid:tyrant2}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant2}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant2}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant2}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant2}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant2}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant2}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant2}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant2}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant2}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant2}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant2}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant2}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant2}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant2}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant2}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant2}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant2}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant2}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant2}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant2}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant2}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant2}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant2}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant2}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant2}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant2}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant2}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant2}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant2}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant2}}, 1, 'Tyrant (look 2)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant2}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant2}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant2}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant2}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant2}}, 0, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 0, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 0, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 1, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 1, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 2, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 3, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 4, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 5, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 6, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 7, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 8, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 9, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 10, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 11, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 12, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 13, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 14, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 15, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 16, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 17, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 18, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 20, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 21, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 22, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 23, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 24, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 25, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 26, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 27, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 28, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 28, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 29, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 30, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 31, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 31, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 31, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 32, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 32, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 33, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 33, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 34, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 35, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 36, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 36, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 37, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 37, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 38, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 39, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 40, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 41, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 42, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 43, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 44, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 44, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 45, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 45, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 46, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 47, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 47, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 47, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 48, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 48, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 48, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 49, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 50, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 51, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 52, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 53, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 54, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 55, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 56, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 57, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 58, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 59, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 60, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 60, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 61, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 61, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 62, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 62, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 63, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 63, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 64, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 64, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 65, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 65, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 66, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 66, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 67, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 67, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 68, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 68, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 69, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 69, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 70, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 70, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 70, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 71, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 71, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 72, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 72, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 73, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 74, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 74, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 74, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 75, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 75, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 75, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 76, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 77, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 78, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 79, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 80, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 81, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 82, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 83, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 84, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 85, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 86, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 87, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 87, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 88, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 88, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 89, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 89, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 90, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 90, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 91, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 91, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 92, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 92, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 93, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 93, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 94, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 94, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 94, 0x0500E002, 0x0500E005)
     , ({{wcid:tyrant2}}, 95, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 95, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 96, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 96, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 97, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 97, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 98, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 98, 0x0500E001, 0x0500E004)
     , ({{wcid:tyrant2}}, 99, 0x0500E000, 0x0500E003)
     , ({{wcid:tyrant2}}, 99, 0x0500E001, 0x0500E004);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant2}}, 1, 40, 0, 0)
     , ({{wcid:tyrant2}}, 2, 40, 0, 0)
     , ({{wcid:tyrant2}}, 3, 40, 0, 0)
     , ({{wcid:tyrant2}}, 4, 40, 0, 0)
     , ({{wcid:tyrant2}}, 5, 10, 0, 0)
     , ({{wcid:tyrant2}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant2}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant2}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant2}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant2}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant2}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant2}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant2}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant2}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant2}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant2}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant2}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant2}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant2}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant2}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant2}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
