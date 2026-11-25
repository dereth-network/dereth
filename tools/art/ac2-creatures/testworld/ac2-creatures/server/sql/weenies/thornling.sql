/* Thornling: an AC2 Thornling as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:thornling}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:thornling}}, 'ac2-thornling', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling}},   1,         16) /* ItemType - Creature */
     , ({{wcid:thornling}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:thornling}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:thornling}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:thornling}},  16,          1) /* ItemUseable - No */
     , ({{wcid:thornling}},  25,          5) /* Level */
     , ({{wcid:thornling}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:thornling}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:thornling}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:thornling}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:thornling}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:thornling}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling}},   1, True ) /* Stuck */
     , ({{wcid:thornling}},  11, False) /* IgnoreCollisions */
     , ({{wcid:thornling}},  12, True ) /* ReportCollisions */
     , ({{wcid:thornling}},  13, False) /* Ethereal */
     , ({{wcid:thornling}},  14, True ) /* GravityStatus */
     , ({{wcid:thornling}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:thornling}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:thornling}},   3,    0.5) /* HealthRate */
     , ({{wcid:thornling}},   4,      5) /* StaminaRate */
     , ({{wcid:thornling}},   5,      1) /* ManaRate */
     , ({{wcid:thornling}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:thornling}},  34,      1) /* PowerupTime */
     , ({{wcid:thornling}},  36,      1) /* ChargeSpeed */
     , ({{wcid:thornling}},  39,    1.0) /* DefaultScale */
     , ({{wcid:thornling}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:thornling}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:thornling}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:thornling}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:thornling}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:thornling}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:thornling}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:thornling}},  64,      1) /* ResistSlash */
     , ({{wcid:thornling}},  65,      1) /* ResistPierce */
     , ({{wcid:thornling}},  66,      1) /* ResistBludgeon */
     , ({{wcid:thornling}},  67,      1) /* ResistFire */
     , ({{wcid:thornling}},  68,      1) /* ResistCold */
     , ({{wcid:thornling}},  69,      1) /* ResistAcid */
     , ({{wcid:thornling}},  70,      1) /* ResistElectric */
     , ({{wcid:thornling}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:thornling}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:thornling}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:thornling}},  74,      1) /* ResistManaDrain */
     , ({{wcid:thornling}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling}}, 1, 'Thornling') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:thornling}},   1, 0x0200EA00) /* Setup */
     , ({{wcid:thornling}},   2, 0x0900EA00) /* MotionTable */
     , ({{wcid:thornling}},   4, 0x3000EA00) /* CombatTable */
     , ({{wcid:thornling}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:thornling}}, 1, 40, 0, 0)
     , ({{wcid:thornling}}, 2, 40, 0, 0)
     , ({{wcid:thornling}}, 3, 40, 0, 0)
     , ({{wcid:thornling}}, 4, 40, 0, 0)
     , ({{wcid:thornling}}, 5, 10, 0, 0)
     , ({{wcid:thornling}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:thornling}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:thornling}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:thornling}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:thornling}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:thornling}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:thornling}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:thornling}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:thornling}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:thornling}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:thornling}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:thornling}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:thornling}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:thornling}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:thornling}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:thornling}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
