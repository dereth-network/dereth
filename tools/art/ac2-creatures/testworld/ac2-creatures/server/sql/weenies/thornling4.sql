/* Thornling (look 4): an AC2 Thornling as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:thornling4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:thornling4}}, 'ac2-thornling4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:thornling4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:thornling4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:thornling4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:thornling4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:thornling4}},  25,          5) /* Level */
     , ({{wcid:thornling4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:thornling4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:thornling4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:thornling4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:thornling4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:thornling4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling4}},   1, True ) /* Stuck */
     , ({{wcid:thornling4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:thornling4}},  12, True ) /* ReportCollisions */
     , ({{wcid:thornling4}},  13, False) /* Ethereal */
     , ({{wcid:thornling4}},  14, True ) /* GravityStatus */
     , ({{wcid:thornling4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:thornling4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:thornling4}},   3,    0.5) /* HealthRate */
     , ({{wcid:thornling4}},   4,      5) /* StaminaRate */
     , ({{wcid:thornling4}},   5,      1) /* ManaRate */
     , ({{wcid:thornling4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:thornling4}},  34,      1) /* PowerupTime */
     , ({{wcid:thornling4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:thornling4}},  39,    1.0) /* DefaultScale */
     , ({{wcid:thornling4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:thornling4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:thornling4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:thornling4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:thornling4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:thornling4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:thornling4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:thornling4}},  64,      1) /* ResistSlash */
     , ({{wcid:thornling4}},  65,      1) /* ResistPierce */
     , ({{wcid:thornling4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:thornling4}},  67,      1) /* ResistFire */
     , ({{wcid:thornling4}},  68,      1) /* ResistCold */
     , ({{wcid:thornling4}},  69,      1) /* ResistAcid */
     , ({{wcid:thornling4}},  70,      1) /* ResistElectric */
     , ({{wcid:thornling4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:thornling4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:thornling4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:thornling4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:thornling4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling4}}, 1, 'Thornling (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling4}},   1, 0x0200EA00) /* Setup */
     , ({{wcid:thornling4}},   2, 0x0900EA00) /* MotionTable */
     , ({{wcid:thornling4}},   4, 0x3000EA00) /* CombatTable */
     , ({{wcid:thornling4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:thornling4}}, 0, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 0, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 1, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 1, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 2, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 3, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 4, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 5, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 6, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 7, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 8, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 9, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 10, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 11, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 12, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 12, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 13, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 13, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 14, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 16, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 16, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 17, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 18, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 19, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 19, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 20, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 21, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 22, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 23, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 24, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 24, 0x0500EA01, 0x0500EA06)
     , ({{wcid:thornling4}}, 25, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 26, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 27, 0x0500EA00, 0x0500EA05)
     , ({{wcid:thornling4}}, 28, 0x0500EA00, 0x0500EA05);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:thornling4}}, 1, 40, 0, 0)
     , ({{wcid:thornling4}}, 2, 40, 0, 0)
     , ({{wcid:thornling4}}, 3, 40, 0, 0)
     , ({{wcid:thornling4}}, 4, 40, 0, 0)
     , ({{wcid:thornling4}}, 5, 10, 0, 0)
     , ({{wcid:thornling4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:thornling4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:thornling4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:thornling4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:thornling4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:thornling4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:thornling4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:thornling4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:thornling4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:thornling4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:thornling4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:thornling4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:thornling4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:thornling4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:thornling4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:thornling4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
