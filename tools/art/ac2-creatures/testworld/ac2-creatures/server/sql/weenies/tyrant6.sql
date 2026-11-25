/* Tyrant (look 6): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant6}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant6}}, 'ac2-tyrant6', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant6}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant6}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant6}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant6}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant6}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant6}},  25,          5) /* Level */
     , ({{wcid:tyrant6}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant6}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant6}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant6}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant6}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant6}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant6}},   1, True ) /* Stuck */
     , ({{wcid:tyrant6}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant6}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant6}},  13, False) /* Ethereal */
     , ({{wcid:tyrant6}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant6}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant6}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant6}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant6}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant6}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant6}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant6}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant6}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant6}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant6}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant6}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant6}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant6}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant6}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant6}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant6}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant6}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant6}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant6}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant6}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant6}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant6}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant6}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant6}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant6}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant6}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant6}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant6}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant6}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant6}}, 1, 'Tyrant (look 6)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant6}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant6}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant6}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant6}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant6}}, 0, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 0, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 0, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 1, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 1, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 2, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 3, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 4, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 5, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 6, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 7, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 8, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 9, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 10, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 11, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 12, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 13, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 14, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 15, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 16, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 17, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 18, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 20, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 21, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 22, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 23, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 24, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 25, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 26, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 27, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 28, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 28, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 29, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 30, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 31, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 31, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 31, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 32, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 32, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 33, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 33, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 34, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 35, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 36, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 36, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 37, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 37, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 38, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 39, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 40, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 41, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 42, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 43, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 44, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 44, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 45, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 45, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 46, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 47, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 47, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 47, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 48, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 48, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 48, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 49, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 50, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 51, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 52, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 53, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 54, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 55, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 56, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 57, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 58, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 59, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 60, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 60, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 61, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 61, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 62, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 62, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 63, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 63, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 64, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 64, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 65, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 65, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 66, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 66, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 67, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 67, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 68, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 68, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 69, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 69, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 70, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 70, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 70, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 71, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 71, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 72, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 72, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 73, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 74, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 74, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 74, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 75, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 75, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 75, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 76, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 77, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 78, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 79, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 80, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 81, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 82, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 83, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 84, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 85, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 86, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 87, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 87, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 88, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 88, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 89, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 89, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 90, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 90, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 91, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 91, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 92, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 92, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 93, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 93, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 94, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 94, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 94, 0x0500E002, 0x0500E00F)
     , ({{wcid:tyrant6}}, 95, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 95, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 96, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 96, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 97, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 97, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 98, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 98, 0x0500E001, 0x0500E00E)
     , ({{wcid:tyrant6}}, 99, 0x0500E000, 0x0500E00D)
     , ({{wcid:tyrant6}}, 99, 0x0500E001, 0x0500E00E);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant6}}, 1, 40, 0, 0)
     , ({{wcid:tyrant6}}, 2, 40, 0, 0)
     , ({{wcid:tyrant6}}, 3, 40, 0, 0)
     , ({{wcid:tyrant6}}, 4, 40, 0, 0)
     , ({{wcid:tyrant6}}, 5, 10, 0, 0)
     , ({{wcid:tyrant6}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant6}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant6}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant6}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant6}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant6}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant6}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant6}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant6}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant6}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant6}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant6}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant6}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant6}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant6}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant6}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
