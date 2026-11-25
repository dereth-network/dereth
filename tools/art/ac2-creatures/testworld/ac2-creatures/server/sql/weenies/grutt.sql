/* Grutt: an AC2 Grutt as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:grutt}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:grutt}}, 'ac2-grutt', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:grutt}},   1,         16) /* ItemType - Creature */
     , ({{wcid:grutt}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:grutt}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:grutt}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:grutt}},  16,          1) /* ItemUseable - No */
     , ({{wcid:grutt}},  25,          5) /* Level */
     , ({{wcid:grutt}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:grutt}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:grutt}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:grutt}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:grutt}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:grutt}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:grutt}},   1, True ) /* Stuck */
     , ({{wcid:grutt}},  11, False) /* IgnoreCollisions */
     , ({{wcid:grutt}},  12, True ) /* ReportCollisions */
     , ({{wcid:grutt}},  13, False) /* Ethereal */
     , ({{wcid:grutt}},  14, True ) /* GravityStatus */
     , ({{wcid:grutt}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:grutt}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:grutt}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:grutt}},   3,    0.5) /* HealthRate */
     , ({{wcid:grutt}},   4,      5) /* StaminaRate */
     , ({{wcid:grutt}},   5,      1) /* ManaRate */
     , ({{wcid:grutt}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:grutt}},  34,      1) /* PowerupTime */
     , ({{wcid:grutt}},  36,      1) /* ChargeSpeed */
     , ({{wcid:grutt}},  39,    1.0) /* DefaultScale */
     , ({{wcid:grutt}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:grutt}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:grutt}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:grutt}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:grutt}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:grutt}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:grutt}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:grutt}},  64,      1) /* ResistSlash */
     , ({{wcid:grutt}},  65,      1) /* ResistPierce */
     , ({{wcid:grutt}},  66,      1) /* ResistBludgeon */
     , ({{wcid:grutt}},  67,      1) /* ResistFire */
     , ({{wcid:grutt}},  68,      1) /* ResistCold */
     , ({{wcid:grutt}},  69,      1) /* ResistAcid */
     , ({{wcid:grutt}},  70,      1) /* ResistElectric */
     , ({{wcid:grutt}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:grutt}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:grutt}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:grutt}},  74,      1) /* ResistManaDrain */
     , ({{wcid:grutt}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:grutt}}, 1, 'Grutt') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:grutt}},   1, 0x0200E600) /* Setup */
     , ({{wcid:grutt}},   2, 0x0900E600) /* MotionTable */
     , ({{wcid:grutt}},   4, 0x3000E600) /* CombatTable */
     , ({{wcid:grutt}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:grutt}}, 1, 40, 0, 0)
     , ({{wcid:grutt}}, 2, 40, 0, 0)
     , ({{wcid:grutt}}, 3, 40, 0, 0)
     , ({{wcid:grutt}}, 4, 40, 0, 0)
     , ({{wcid:grutt}}, 5, 10, 0, 0)
     , ({{wcid:grutt}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:grutt}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:grutt}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:grutt}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:grutt}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:grutt}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:grutt}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:grutt}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:grutt}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:grutt}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:grutt}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:grutt}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:grutt}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:grutt}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:grutt}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:grutt}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
