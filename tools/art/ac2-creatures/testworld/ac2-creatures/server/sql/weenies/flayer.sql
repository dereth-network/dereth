/* Flayer: an AC2 Flayer as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:flayer}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:flayer}}, 'ac2-flayer', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:flayer}},   1,         16) /* ItemType - Creature */
     , ({{wcid:flayer}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:flayer}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:flayer}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:flayer}},  16,          1) /* ItemUseable - No */
     , ({{wcid:flayer}},  25,          5) /* Level */
     , ({{wcid:flayer}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:flayer}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:flayer}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:flayer}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:flayer}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:flayer}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:flayer}},   1, True ) /* Stuck */
     , ({{wcid:flayer}},  11, False) /* IgnoreCollisions */
     , ({{wcid:flayer}},  12, True ) /* ReportCollisions */
     , ({{wcid:flayer}},  13, False) /* Ethereal */
     , ({{wcid:flayer}},  14, True ) /* GravityStatus */
     , ({{wcid:flayer}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:flayer}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:flayer}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:flayer}},   3,    0.5) /* HealthRate */
     , ({{wcid:flayer}},   4,      5) /* StaminaRate */
     , ({{wcid:flayer}},   5,      1) /* ManaRate */
     , ({{wcid:flayer}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:flayer}},  34,      1) /* PowerupTime */
     , ({{wcid:flayer}},  36,      1) /* ChargeSpeed */
     , ({{wcid:flayer}},  39,    0.7) /* DefaultScale */
     , ({{wcid:flayer}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:flayer}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:flayer}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:flayer}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:flayer}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:flayer}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:flayer}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:flayer}},  64,      1) /* ResistSlash */
     , ({{wcid:flayer}},  65,      1) /* ResistPierce */
     , ({{wcid:flayer}},  66,      1) /* ResistBludgeon */
     , ({{wcid:flayer}},  67,      1) /* ResistFire */
     , ({{wcid:flayer}},  68,      1) /* ResistCold */
     , ({{wcid:flayer}},  69,      1) /* ResistAcid */
     , ({{wcid:flayer}},  70,      1) /* ResistElectric */
     , ({{wcid:flayer}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:flayer}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:flayer}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:flayer}},  74,      1) /* ResistManaDrain */
     , ({{wcid:flayer}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:flayer}}, 1, 'Flayer') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:flayer}},   1, 0x0200E400) /* Setup */
     , ({{wcid:flayer}},   2, 0x0900E400) /* MotionTable */
     , ({{wcid:flayer}},   4, 0x3000E400) /* CombatTable */
     , ({{wcid:flayer}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:flayer}}, 1, 40, 0, 0)
     , ({{wcid:flayer}}, 2, 40, 0, 0)
     , ({{wcid:flayer}}, 3, 40, 0, 0)
     , ({{wcid:flayer}}, 4, 40, 0, 0)
     , ({{wcid:flayer}}, 5, 10, 0, 0)
     , ({{wcid:flayer}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:flayer}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:flayer}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:flayer}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:flayer}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:flayer}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:flayer}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:flayer}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:flayer}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:flayer}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:flayer}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:flayer}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:flayer}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:flayer}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:flayer}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:flayer}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
