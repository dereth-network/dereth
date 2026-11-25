/* Tyrant (look 3): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant3}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant3}}, 'ac2-tyrant3', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant3}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant3}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant3}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant3}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant3}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant3}},  25,          5) /* Level */
     , ({{wcid:tyrant3}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant3}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant3}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant3}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant3}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant3}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant3}},   1, True ) /* Stuck */
     , ({{wcid:tyrant3}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant3}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant3}},  13, False) /* Ethereal */
     , ({{wcid:tyrant3}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant3}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant3}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant3}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant3}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant3}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant3}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant3}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant3}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant3}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant3}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant3}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant3}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant3}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant3}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant3}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant3}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant3}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant3}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant3}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant3}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant3}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant3}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant3}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant3}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant3}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant3}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant3}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant3}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant3}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant3}}, 1, 'Tyrant (look 3)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant3}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant3}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant3}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant3}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant3}}, 0, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 0, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 0, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 1, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 1, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 2, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 3, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 4, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 5, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 6, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 7, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 8, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 9, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 10, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 11, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 12, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 13, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 14, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 15, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 16, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 17, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 18, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 20, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 21, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 22, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 23, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 24, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 25, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 26, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 27, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 28, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 28, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 29, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 30, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 31, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 31, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 31, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 32, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 32, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 33, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 33, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 34, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 35, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 36, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 36, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 37, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 37, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 38, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 39, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 40, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 41, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 42, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 43, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 44, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 44, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 45, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 45, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 46, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 47, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 47, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 47, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 48, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 48, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 48, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 49, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 50, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 51, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 52, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 53, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 54, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 55, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 56, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 57, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 58, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 59, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 60, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 60, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 61, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 61, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 62, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 62, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 63, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 63, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 64, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 64, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 65, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 65, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 66, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 66, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 67, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 67, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 68, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 68, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 69, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 69, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 70, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 70, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 70, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 71, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 71, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 72, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 72, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 73, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 74, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 74, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 74, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 75, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 75, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 75, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 76, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 77, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 78, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 79, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 80, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 81, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 82, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 83, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 84, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 85, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 86, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 87, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 87, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 88, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 88, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 89, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 89, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 90, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 90, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 91, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 91, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 92, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 92, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 93, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 93, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 94, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 94, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 94, 0x0500E002, 0x0500E008)
     , ({{wcid:tyrant3}}, 95, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 95, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 96, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 96, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 97, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 97, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 98, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 98, 0x0500E001, 0x0500E007)
     , ({{wcid:tyrant3}}, 99, 0x0500E000, 0x0500E006)
     , ({{wcid:tyrant3}}, 99, 0x0500E001, 0x0500E007);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant3}}, 1, 40, 0, 0)
     , ({{wcid:tyrant3}}, 2, 40, 0, 0)
     , ({{wcid:tyrant3}}, 3, 40, 0, 0)
     , ({{wcid:tyrant3}}, 4, 40, 0, 0)
     , ({{wcid:tyrant3}}, 5, 10, 0, 0)
     , ({{wcid:tyrant3}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant3}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant3}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant3}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant3}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant3}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant3}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant3}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant3}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant3}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant3}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant3}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant3}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant3}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant3}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant3}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
