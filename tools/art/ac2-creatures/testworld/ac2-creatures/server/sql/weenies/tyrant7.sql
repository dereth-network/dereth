/* Tyrant (look 7): an AC2 Tyrant as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:tyrant7}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:tyrant7}}, 'ac2-tyrant7', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant7}},   1,         16) /* ItemType - Creature */
     , ({{wcid:tyrant7}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:tyrant7}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:tyrant7}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:tyrant7}},  16,          1) /* ItemUseable - No */
     , ({{wcid:tyrant7}},  25,          5) /* Level */
     , ({{wcid:tyrant7}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:tyrant7}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:tyrant7}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:tyrant7}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:tyrant7}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:tyrant7}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant7}},   1, True ) /* Stuck */
     , ({{wcid:tyrant7}},  11, False) /* IgnoreCollisions */
     , ({{wcid:tyrant7}},  12, True ) /* ReportCollisions */
     , ({{wcid:tyrant7}},  13, False) /* Ethereal */
     , ({{wcid:tyrant7}},  14, True ) /* GravityStatus */
     , ({{wcid:tyrant7}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant7}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:tyrant7}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:tyrant7}},   3,    0.5) /* HealthRate */
     , ({{wcid:tyrant7}},   4,      5) /* StaminaRate */
     , ({{wcid:tyrant7}},   5,      1) /* ManaRate */
     , ({{wcid:tyrant7}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:tyrant7}},  34,      1) /* PowerupTime */
     , ({{wcid:tyrant7}},  36,      1) /* ChargeSpeed */
     , ({{wcid:tyrant7}},  39,   0.45) /* DefaultScale */
     , ({{wcid:tyrant7}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:tyrant7}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:tyrant7}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:tyrant7}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:tyrant7}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:tyrant7}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:tyrant7}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:tyrant7}},  64,      1) /* ResistSlash */
     , ({{wcid:tyrant7}},  65,      1) /* ResistPierce */
     , ({{wcid:tyrant7}},  66,      1) /* ResistBludgeon */
     , ({{wcid:tyrant7}},  67,      1) /* ResistFire */
     , ({{wcid:tyrant7}},  68,      1) /* ResistCold */
     , ({{wcid:tyrant7}},  69,      1) /* ResistAcid */
     , ({{wcid:tyrant7}},  70,      1) /* ResistElectric */
     , ({{wcid:tyrant7}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:tyrant7}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:tyrant7}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:tyrant7}},  74,      1) /* ResistManaDrain */
     , ({{wcid:tyrant7}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant7}}, 1, 'Tyrant (look 7)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:tyrant7}},   1, 0x0200E000) /* Setup */
     , ({{wcid:tyrant7}},   2, 0x0900E000) /* MotionTable */
     , ({{wcid:tyrant7}},   4, 0x3000E000) /* CombatTable */
     , ({{wcid:tyrant7}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:tyrant7}}, 0, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 0, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 0, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 1, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 1, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 2, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 3, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 4, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 5, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 6, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 7, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 8, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 9, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 10, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 11, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 12, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 13, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 14, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 15, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 16, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 17, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 18, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 20, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 21, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 22, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 23, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 24, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 25, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 26, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 27, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 28, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 28, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 29, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 30, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 31, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 31, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 31, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 32, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 32, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 33, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 33, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 34, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 35, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 36, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 36, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 37, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 37, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 38, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 39, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 40, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 41, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 42, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 43, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 44, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 44, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 45, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 45, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 46, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 47, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 47, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 47, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 48, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 48, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 48, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 49, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 50, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 51, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 52, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 53, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 54, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 55, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 56, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 57, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 58, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 59, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 60, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 60, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 61, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 61, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 62, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 62, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 63, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 63, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 64, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 64, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 65, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 65, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 66, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 66, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 67, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 67, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 68, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 68, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 69, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 69, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 70, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 70, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 70, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 71, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 71, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 72, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 72, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 73, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 74, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 74, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 74, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 75, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 75, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 75, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 76, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 77, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 78, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 79, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 80, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 81, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 82, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 83, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 84, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 85, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 86, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 87, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 87, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 88, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 88, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 89, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 89, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 90, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 90, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 91, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 91, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 92, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 92, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 93, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 93, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 94, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 94, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 94, 0x0500E002, 0x0500E012)
     , ({{wcid:tyrant7}}, 95, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 95, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 96, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 96, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 97, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 97, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 98, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 98, 0x0500E001, 0x0500E011)
     , ({{wcid:tyrant7}}, 99, 0x0500E000, 0x0500E010)
     , ({{wcid:tyrant7}}, 99, 0x0500E001, 0x0500E011);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:tyrant7}}, 1, 40, 0, 0)
     , ({{wcid:tyrant7}}, 2, 40, 0, 0)
     , ({{wcid:tyrant7}}, 3, 40, 0, 0)
     , ({{wcid:tyrant7}}, 4, 40, 0, 0)
     , ({{wcid:tyrant7}}, 5, 10, 0, 0)
     , ({{wcid:tyrant7}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:tyrant7}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:tyrant7}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:tyrant7}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:tyrant7}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:tyrant7}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:tyrant7}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:tyrant7}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:tyrant7}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:tyrant7}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:tyrant7}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:tyrant7}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:tyrant7}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:tyrant7}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:tyrant7}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:tyrant7}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
