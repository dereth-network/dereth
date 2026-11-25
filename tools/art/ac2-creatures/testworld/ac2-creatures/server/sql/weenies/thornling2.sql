/* Thornling (look 2): an AC2 Thornling as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:thornling2}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:thornling2}}, 'ac2-thornling2', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling2}},   1,         16) /* ItemType - Creature */
     , ({{wcid:thornling2}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:thornling2}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:thornling2}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:thornling2}},  16,          1) /* ItemUseable - No */
     , ({{wcid:thornling2}},  25,          5) /* Level */
     , ({{wcid:thornling2}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:thornling2}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:thornling2}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:thornling2}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:thornling2}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:thornling2}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling2}},   1, True ) /* Stuck */
     , ({{wcid:thornling2}},  11, False) /* IgnoreCollisions */
     , ({{wcid:thornling2}},  12, True ) /* ReportCollisions */
     , ({{wcid:thornling2}},  13, False) /* Ethereal */
     , ({{wcid:thornling2}},  14, True ) /* GravityStatus */
     , ({{wcid:thornling2}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling2}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:thornling2}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:thornling2}},   3,    0.5) /* HealthRate */
     , ({{wcid:thornling2}},   4,      5) /* StaminaRate */
     , ({{wcid:thornling2}},   5,      1) /* ManaRate */
     , ({{wcid:thornling2}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:thornling2}},  34,      1) /* PowerupTime */
     , ({{wcid:thornling2}},  36,      1) /* ChargeSpeed */
     , ({{wcid:thornling2}},  39,    1.0) /* DefaultScale */
     , ({{wcid:thornling2}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:thornling2}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:thornling2}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:thornling2}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:thornling2}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:thornling2}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:thornling2}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:thornling2}},  64,      1) /* ResistSlash */
     , ({{wcid:thornling2}},  65,      1) /* ResistPierce */
     , ({{wcid:thornling2}},  66,      1) /* ResistBludgeon */
     , ({{wcid:thornling2}},  67,      1) /* ResistFire */
     , ({{wcid:thornling2}},  68,      1) /* ResistCold */
     , ({{wcid:thornling2}},  69,      1) /* ResistAcid */
     , ({{wcid:thornling2}},  70,      1) /* ResistElectric */
     , ({{wcid:thornling2}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:thornling2}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:thornling2}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:thornling2}},  74,      1) /* ResistManaDrain */
     , ({{wcid:thornling2}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling2}}, 1, 'Thornling (look 2)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling2}},   1, 0x0200EA00) /* Setup */
     , ({{wcid:thornling2}},   2, 0x0900EA00) /* MotionTable */
     , ({{wcid:thornling2}},   4, 0x3000EA00) /* CombatTable */
     , ({{wcid:thornling2}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:thornling2}}, 0, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 0, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 1, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 1, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 2, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 3, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 4, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 5, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 6, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 7, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 8, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 9, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 10, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 11, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 12, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 12, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 13, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 13, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 14, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 16, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 16, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 17, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 18, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 19, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 19, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 20, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 21, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 22, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 23, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 24, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 24, 0x0500EA01, 0x0500EA03)
     , ({{wcid:thornling2}}, 25, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 26, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 27, 0x0500EA00, 0x0500EA02)
     , ({{wcid:thornling2}}, 28, 0x0500EA00, 0x0500EA02);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:thornling2}}, 1, 40, 0, 0)
     , ({{wcid:thornling2}}, 2, 40, 0, 0)
     , ({{wcid:thornling2}}, 3, 40, 0, 0)
     , ({{wcid:thornling2}}, 4, 40, 0, 0)
     , ({{wcid:thornling2}}, 5, 10, 0, 0)
     , ({{wcid:thornling2}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:thornling2}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:thornling2}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:thornling2}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:thornling2}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:thornling2}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:thornling2}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:thornling2}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:thornling2}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:thornling2}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:thornling2}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:thornling2}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:thornling2}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:thornling2}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:thornling2}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:thornling2}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
