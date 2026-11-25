/* Beetle: an AC2 Beetle as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:beetle}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:beetle}}, 'ac2-beetle', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle}},   1,         16) /* ItemType - Creature */
     , ({{wcid:beetle}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:beetle}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:beetle}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:beetle}},  16,          1) /* ItemUseable - No */
     , ({{wcid:beetle}},  25,          5) /* Level */
     , ({{wcid:beetle}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:beetle}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:beetle}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:beetle}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:beetle}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:beetle}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle}},   1, True ) /* Stuck */
     , ({{wcid:beetle}},  11, False) /* IgnoreCollisions */
     , ({{wcid:beetle}},  12, True ) /* ReportCollisions */
     , ({{wcid:beetle}},  13, False) /* Ethereal */
     , ({{wcid:beetle}},  14, True ) /* GravityStatus */
     , ({{wcid:beetle}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:beetle}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:beetle}},   3,    0.5) /* HealthRate */
     , ({{wcid:beetle}},   4,      5) /* StaminaRate */
     , ({{wcid:beetle}},   5,      1) /* ManaRate */
     , ({{wcid:beetle}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:beetle}},  34,      1) /* PowerupTime */
     , ({{wcid:beetle}},  36,      1) /* ChargeSpeed */
     , ({{wcid:beetle}},  39,    1.0) /* DefaultScale */
     , ({{wcid:beetle}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:beetle}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:beetle}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:beetle}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:beetle}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:beetle}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:beetle}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:beetle}},  64,      1) /* ResistSlash */
     , ({{wcid:beetle}},  65,      1) /* ResistPierce */
     , ({{wcid:beetle}},  66,      1) /* ResistBludgeon */
     , ({{wcid:beetle}},  67,      1) /* ResistFire */
     , ({{wcid:beetle}},  68,      1) /* ResistCold */
     , ({{wcid:beetle}},  69,      1) /* ResistAcid */
     , ({{wcid:beetle}},  70,      1) /* ResistElectric */
     , ({{wcid:beetle}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:beetle}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:beetle}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:beetle}},  74,      1) /* ResistManaDrain */
     , ({{wcid:beetle}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle}}, 1, 'Beetle') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle}},   1, 0x0200E100) /* Setup */
     , ({{wcid:beetle}},   2, 0x0900E100) /* MotionTable */
     , ({{wcid:beetle}},   4, 0x3000E100) /* CombatTable */
     , ({{wcid:beetle}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:beetle}}, 1, 40, 0, 0)
     , ({{wcid:beetle}}, 2, 40, 0, 0)
     , ({{wcid:beetle}}, 3, 40, 0, 0)
     , ({{wcid:beetle}}, 4, 40, 0, 0)
     , ({{wcid:beetle}}, 5, 10, 0, 0)
     , ({{wcid:beetle}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:beetle}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:beetle}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:beetle}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:beetle}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:beetle}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:beetle}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:beetle}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:beetle}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:beetle}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:beetle}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:beetle}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:beetle}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:beetle}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:beetle}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:beetle}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
