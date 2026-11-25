/* Scraven: an AC2 Scraven as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:scraven}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:scraven}}, 'ac2-scraven', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven}},   1,         16) /* ItemType - Creature */
     , ({{wcid:scraven}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:scraven}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:scraven}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:scraven}},  16,          1) /* ItemUseable - No */
     , ({{wcid:scraven}},  25,          5) /* Level */
     , ({{wcid:scraven}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:scraven}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:scraven}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:scraven}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:scraven}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:scraven}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven}},   1, True ) /* Stuck */
     , ({{wcid:scraven}},  11, False) /* IgnoreCollisions */
     , ({{wcid:scraven}},  12, True ) /* ReportCollisions */
     , ({{wcid:scraven}},  13, False) /* Ethereal */
     , ({{wcid:scraven}},  14, True ) /* GravityStatus */
     , ({{wcid:scraven}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:scraven}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:scraven}},   3,    0.5) /* HealthRate */
     , ({{wcid:scraven}},   4,      5) /* StaminaRate */
     , ({{wcid:scraven}},   5,      1) /* ManaRate */
     , ({{wcid:scraven}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:scraven}},  34,      1) /* PowerupTime */
     , ({{wcid:scraven}},  36,      1) /* ChargeSpeed */
     , ({{wcid:scraven}},  39,    1.0) /* DefaultScale */
     , ({{wcid:scraven}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:scraven}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:scraven}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:scraven}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:scraven}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:scraven}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:scraven}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:scraven}},  64,      1) /* ResistSlash */
     , ({{wcid:scraven}},  65,      1) /* ResistPierce */
     , ({{wcid:scraven}},  66,      1) /* ResistBludgeon */
     , ({{wcid:scraven}},  67,      1) /* ResistFire */
     , ({{wcid:scraven}},  68,      1) /* ResistCold */
     , ({{wcid:scraven}},  69,      1) /* ResistAcid */
     , ({{wcid:scraven}},  70,      1) /* ResistElectric */
     , ({{wcid:scraven}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:scraven}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:scraven}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:scraven}},  74,      1) /* ResistManaDrain */
     , ({{wcid:scraven}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven}}, 1, 'Scraven') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven}},   1, 0x0200E900) /* Setup */
     , ({{wcid:scraven}},   2, 0x0900E900) /* MotionTable */
     , ({{wcid:scraven}},   4, 0x3000E900) /* CombatTable */
     , ({{wcid:scraven}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:scraven}}, 1, 40, 0, 0)
     , ({{wcid:scraven}}, 2, 40, 0, 0)
     , ({{wcid:scraven}}, 3, 40, 0, 0)
     , ({{wcid:scraven}}, 4, 40, 0, 0)
     , ({{wcid:scraven}}, 5, 10, 0, 0)
     , ({{wcid:scraven}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:scraven}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:scraven}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:scraven}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:scraven}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:scraven}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:scraven}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:scraven}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:scraven}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:scraven}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:scraven}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:scraven}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:scraven}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:scraven}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:scraven}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:scraven}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
