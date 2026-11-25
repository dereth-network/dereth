/* Crone (look 6): an AC2 Crone as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:crone6}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:crone6}}, 'ac2-crone6', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:crone6}},   1,         16) /* ItemType - Creature */
     , ({{wcid:crone6}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:crone6}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:crone6}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:crone6}},  16,          1) /* ItemUseable - No */
     , ({{wcid:crone6}},  25,          5) /* Level */
     , ({{wcid:crone6}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:crone6}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:crone6}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:crone6}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:crone6}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:crone6}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:crone6}},   1, True ) /* Stuck */
     , ({{wcid:crone6}},  11, False) /* IgnoreCollisions */
     , ({{wcid:crone6}},  12, True ) /* ReportCollisions */
     , ({{wcid:crone6}},  13, False) /* Ethereal */
     , ({{wcid:crone6}},  14, True ) /* GravityStatus */
     , ({{wcid:crone6}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:crone6}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:crone6}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:crone6}},   3,    0.5) /* HealthRate */
     , ({{wcid:crone6}},   4,      5) /* StaminaRate */
     , ({{wcid:crone6}},   5,      1) /* ManaRate */
     , ({{wcid:crone6}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:crone6}},  34,      1) /* PowerupTime */
     , ({{wcid:crone6}},  36,      1) /* ChargeSpeed */
     , ({{wcid:crone6}},  39,    1.4) /* DefaultScale */
     , ({{wcid:crone6}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:crone6}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:crone6}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:crone6}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:crone6}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:crone6}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:crone6}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:crone6}},  64,      1) /* ResistSlash */
     , ({{wcid:crone6}},  65,      1) /* ResistPierce */
     , ({{wcid:crone6}},  66,      1) /* ResistBludgeon */
     , ({{wcid:crone6}},  67,      1) /* ResistFire */
     , ({{wcid:crone6}},  68,      1) /* ResistCold */
     , ({{wcid:crone6}},  69,      1) /* ResistAcid */
     , ({{wcid:crone6}},  70,      1) /* ResistElectric */
     , ({{wcid:crone6}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:crone6}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:crone6}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:crone6}},  74,      1) /* ResistManaDrain */
     , ({{wcid:crone6}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:crone6}}, 1, 'Crone (look 6)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:crone6}},   1, 0x0200E200) /* Setup */
     , ({{wcid:crone6}},   2, 0x0900E200) /* MotionTable */
     , ({{wcid:crone6}},   4, 0x3000E200) /* CombatTable */
     , ({{wcid:crone6}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:crone6}}, 0, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 1, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 2, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 3, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 4, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 5, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 6, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 7, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 8, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 9, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 10, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 11, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 12, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 13, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 15, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 16, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 17, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 18, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 19, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 20, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 21, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 22, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 23, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 24, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 25, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 26, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 27, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 28, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 29, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 30, 0x0500E200, 0x0500E205)
     , ({{wcid:crone6}}, 31, 0x0500E200, 0x0500E205);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:crone6}}, 1, 40, 0, 0)
     , ({{wcid:crone6}}, 2, 40, 0, 0)
     , ({{wcid:crone6}}, 3, 40, 0, 0)
     , ({{wcid:crone6}}, 4, 40, 0, 0)
     , ({{wcid:crone6}}, 5, 10, 0, 0)
     , ({{wcid:crone6}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:crone6}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:crone6}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:crone6}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:crone6}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:crone6}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:crone6}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:crone6}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:crone6}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:crone6}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:crone6}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:crone6}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:crone6}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:crone6}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:crone6}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:crone6}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
