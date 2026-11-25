/* Tyrant (look 5): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant5}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant5}}, 'ac2-tyrant5', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant5}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant5}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant5}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant5}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant5}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant5}},  25,          5) /* Level */
     , ({{wcid:tyrant5}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant5}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant5}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant5}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant5}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant5}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant5}},   1, True ) /* Stuck */
     , ({{wcid:tyrant5}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant5}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant5}},  13, False) /* Ethereal */
     , ({{wcid:tyrant5}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant5}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant5}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant5}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant5}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant5}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant5}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant5}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant5}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant5}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant5}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant5}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant5}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant5}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant5}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant5}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant5}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant5}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant5}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant5}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant5}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant5}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant5}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant5}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant5}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant5}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant5}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant5}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant5}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant5}},  75,      1) /* ResistManaBoost */
     , ({{wcid:tyrant5}},  76,    0.6) /* Translucency */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant5}}, 1, 'Tyrant (look 5)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant5}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant5}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant5}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant5}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant5}}, 0, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 0, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 0, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 1, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 1, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 2, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 3, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 4, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 5, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 6, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 7, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 8, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 9, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 10, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 11, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 12, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 13, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 14, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 15, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 16, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 17, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 18, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 20, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 21, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 22, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 23, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 24, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 25, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 26, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 27, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 28, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 28, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 29, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 30, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 31, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 31, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 31, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 32, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 32, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 33, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 33, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 34, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 35, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 36, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 36, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 37, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 37, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 38, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 39, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 40, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 41, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 42, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 43, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 44, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 44, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 45, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 45, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 46, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 47, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 47, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 47, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 48, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 48, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 48, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 49, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 50, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 51, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 52, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 53, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 54, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 55, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 56, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 57, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 58, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 59, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 60, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 60, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 61, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 61, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 62, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 62, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 63, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 63, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 64, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 64, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 65, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 65, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 66, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 66, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 67, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 67, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 68, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 68, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 69, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 69, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 70, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 70, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 70, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 71, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 71, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 72, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 72, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 73, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 74, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 74, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 74, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 75, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 75, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 75, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 76, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 77, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 78, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 79, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 80, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 81, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 82, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 83, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 84, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 85, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 86, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 87, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 87, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 88, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 88, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 89, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 89, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 90, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 90, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 91, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 91, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 92, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 92, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 93, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 93, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 94, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 94, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 94, 0x0500E002, 0x0500E00C)
     , ({{wcid:tyrant5}}, 95, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 95, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 96, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 96, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 97, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 97, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 98, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 98, 0x0500E001, 0x0500E00C)
     , ({{wcid:tyrant5}}, 99, 0x0500E000, 0x0500E00C)
     , ({{wcid:tyrant5}}, 99, 0x0500E001, 0x0500E00C);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant5}}, 1, 40, 0, 0)
     , ({{wcid:tyrant5}}, 2, 40, 0, 0)
     , ({{wcid:tyrant5}}, 3, 40, 0, 0)
     , ({{wcid:tyrant5}}, 4, 40, 0, 0)
     , ({{wcid:tyrant5}}, 5, 10, 0, 0)
     , ({{wcid:tyrant5}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant5}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant5}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant5}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant5}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant5}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant5}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant5}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant5}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant5}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant5}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant5}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant5}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant5}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant5}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant5}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
