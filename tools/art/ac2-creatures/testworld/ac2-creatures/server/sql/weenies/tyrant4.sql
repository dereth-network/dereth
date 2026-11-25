/* Tyrant (look 4): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant4}}, 'ac2-tyrant4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant4}},  25,          5) /* Level */
     , ({{wcid:tyrant4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant4}},   1, True ) /* Stuck */
     , ({{wcid:tyrant4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant4}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant4}},  13, False) /* Ethereal */
     , ({{wcid:tyrant4}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant4}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant4}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant4}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant4}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant4}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant4}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant4}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant4}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant4}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant4}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant4}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant4}}, 1, 'Tyrant (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant4}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant4}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant4}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant4}}, 0, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 0, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 0, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 1, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 1, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 2, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 3, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 4, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 5, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 6, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 7, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 8, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 9, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 10, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 11, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 12, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 13, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 14, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 15, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 16, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 17, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 18, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 20, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 21, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 22, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 23, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 24, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 25, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 26, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 27, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 28, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 28, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 29, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 30, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 31, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 31, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 31, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 32, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 32, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 33, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 33, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 34, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 35, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 36, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 36, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 37, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 37, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 38, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 39, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 40, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 41, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 42, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 43, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 44, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 44, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 45, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 45, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 46, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 47, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 47, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 47, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 48, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 48, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 48, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 49, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 50, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 51, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 52, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 53, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 54, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 55, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 56, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 57, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 58, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 59, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 60, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 60, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 61, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 61, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 62, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 62, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 63, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 63, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 64, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 64, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 65, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 65, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 66, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 66, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 67, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 67, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 68, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 68, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 69, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 69, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 70, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 70, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 70, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 71, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 71, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 72, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 72, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 73, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 74, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 74, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 74, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 75, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 75, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 75, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 76, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 77, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 78, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 79, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 80, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 81, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 82, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 83, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 84, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 85, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 86, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 87, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 87, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 88, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 88, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 89, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 89, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 90, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 90, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 91, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 91, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 92, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 92, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 93, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 93, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 94, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 94, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 94, 0x0500E002, 0x0500E00B)
     , ({{wcid:tyrant4}}, 95, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 95, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 96, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 96, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 97, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 97, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 98, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 98, 0x0500E001, 0x0500E00A)
     , ({{wcid:tyrant4}}, 99, 0x0500E000, 0x0500E009)
     , ({{wcid:tyrant4}}, 99, 0x0500E001, 0x0500E00A);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant4}}, 1, 40, 0, 0)
     , ({{wcid:tyrant4}}, 2, 40, 0, 0)
     , ({{wcid:tyrant4}}, 3, 40, 0, 0)
     , ({{wcid:tyrant4}}, 4, 40, 0, 0)
     , ({{wcid:tyrant4}}, 5, 10, 0, 0)
     , ({{wcid:tyrant4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
