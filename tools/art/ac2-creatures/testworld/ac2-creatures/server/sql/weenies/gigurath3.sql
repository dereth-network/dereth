/* Gigurath (look 3): an AC2 Gigurath as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:gigurath3}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:gigurath3}}, 'ac2-gigurath3', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gigurath3}},   1,         16) /* ItemType - Creature */
     , ({{wcid:gigurath3}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:gigurath3}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:gigurath3}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:gigurath3}},  16,          1) /* ItemUseable - No */
     , ({{wcid:gigurath3}},  25,          5) /* Level */
     , ({{wcid:gigurath3}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:gigurath3}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:gigurath3}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:gigurath3}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:gigurath3}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:gigurath3}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gigurath3}},   1, True ) /* Stuck */
     , ({{wcid:gigurath3}},  11, False) /* IgnoreCollisions */
     , ({{wcid:gigurath3}},  12, True ) /* ReportCollisions */
     , ({{wcid:gigurath3}},  13, False) /* Ethereal */
     , ({{wcid:gigurath3}},  14, True ) /* GravityStatus */
     , ({{wcid:gigurath3}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gigurath3}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:gigurath3}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:gigurath3}},   3,    0.5) /* HealthRate */
     , ({{wcid:gigurath3}},   4,      5) /* StaminaRate */
     , ({{wcid:gigurath3}},   5,      1) /* ManaRate */
     , ({{wcid:gigurath3}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:gigurath3}},  34,      1) /* PowerupTime */
     , ({{wcid:gigurath3}},  36,      1) /* ChargeSpeed */
     , ({{wcid:gigurath3}},  39,    1.0) /* DefaultScale */
     , ({{wcid:gigurath3}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:gigurath3}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:gigurath3}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:gigurath3}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:gigurath3}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:gigurath3}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:gigurath3}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:gigurath3}},  64,      1) /* ResistSlash */
     , ({{wcid:gigurath3}},  65,      1) /* ResistPierce */
     , ({{wcid:gigurath3}},  66,      1) /* ResistBludgeon */
     , ({{wcid:gigurath3}},  67,      1) /* ResistFire */
     , ({{wcid:gigurath3}},  68,      1) /* ResistCold */
     , ({{wcid:gigurath3}},  69,      1) /* ResistAcid */
     , ({{wcid:gigurath3}},  70,      1) /* ResistElectric */
     , ({{wcid:gigurath3}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:gigurath3}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:gigurath3}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:gigurath3}},  74,      1) /* ResistManaDrain */
     , ({{wcid:gigurath3}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gigurath3}}, 1, 'Gigurath (look 3)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:gigurath3}},   1, 0x0200E500) /* Setup */
     , ({{wcid:gigurath3}},   2, 0x0900E500) /* MotionTable */
     , ({{wcid:gigurath3}},   4, 0x3000E500) /* CombatTable */
     , ({{wcid:gigurath3}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:gigurath3}}, 0, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 0, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 0, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 1, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 1, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 1, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 2, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 2, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 3, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 3, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 3, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 4, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 4, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 5, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 5, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 6, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 6, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 7, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 7, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 8, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 9, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 10, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 11, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 12, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 13, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 13, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 14, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 15, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 15, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 16, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 16, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 17, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 18, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 19, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 20, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 21, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 21, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 22, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 22, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 23, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 24, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 25, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 25, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 26, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 27, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 28, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 29, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 29, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 30, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 31, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 32, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 33, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 33, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 33, 0x0500E502, 0x0500E508)
     , ({{wcid:gigurath3}}, 34, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 34, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 35, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 36, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 37, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 38, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 39, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 39, 0x0500E501, 0x0500E507)
     , ({{wcid:gigurath3}}, 40, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 41, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 42, 0x0500E500, 0x0500E506)
     , ({{wcid:gigurath3}}, 43, 0x0500E500, 0x0500E506);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:gigurath3}}, 1, 40, 0, 0)
     , ({{wcid:gigurath3}}, 2, 40, 0, 0)
     , ({{wcid:gigurath3}}, 3, 40, 0, 0)
     , ({{wcid:gigurath3}}, 4, 40, 0, 0)
     , ({{wcid:gigurath3}}, 5, 10, 0, 0)
     , ({{wcid:gigurath3}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:gigurath3}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:gigurath3}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:gigurath3}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:gigurath3}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:gigurath3}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:gigurath3}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:gigurath3}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:gigurath3}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:gigurath3}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:gigurath3}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:gigurath3}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:gigurath3}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:gigurath3}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:gigurath3}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:gigurath3}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
