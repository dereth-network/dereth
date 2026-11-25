/* Mucor: an AC2 Mucor as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:mucor}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:mucor}}, 'ac2-mucor', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor}},   1,         16) /* ItemType - Creature */
     , ({{wcid:mucor}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:mucor}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:mucor}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:mucor}},  16,          1) /* ItemUseable - No */
     , ({{wcid:mucor}},  25,          5) /* Level */
     , ({{wcid:mucor}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:mucor}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:mucor}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:mucor}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:mucor}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:mucor}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor}},   1, True ) /* Stuck */
     , ({{wcid:mucor}},  11, False) /* IgnoreCollisions */
     , ({{wcid:mucor}},  12, True ) /* ReportCollisions */
     , ({{wcid:mucor}},  13, False) /* Ethereal */
     , ({{wcid:mucor}},  14, True ) /* GravityStatus */
     , ({{wcid:mucor}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:mucor}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:mucor}},   3,    0.5) /* HealthRate */
     , ({{wcid:mucor}},   4,      5) /* StaminaRate */
     , ({{wcid:mucor}},   5,      1) /* ManaRate */
     , ({{wcid:mucor}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:mucor}},  34,      1) /* PowerupTime */
     , ({{wcid:mucor}},  36,      1) /* ChargeSpeed */
     , ({{wcid:mucor}},  39,   0.75) /* DefaultScale */
     , ({{wcid:mucor}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:mucor}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:mucor}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:mucor}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:mucor}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:mucor}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:mucor}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:mucor}},  64,      1) /* ResistSlash */
     , ({{wcid:mucor}},  65,      1) /* ResistPierce */
     , ({{wcid:mucor}},  66,      1) /* ResistBludgeon */
     , ({{wcid:mucor}},  67,      1) /* ResistFire */
     , ({{wcid:mucor}},  68,      1) /* ResistCold */
     , ({{wcid:mucor}},  69,      1) /* ResistAcid */
     , ({{wcid:mucor}},  70,      1) /* ResistElectric */
     , ({{wcid:mucor}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:mucor}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:mucor}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:mucor}},  74,      1) /* ResistManaDrain */
     , ({{wcid:mucor}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor}}, 1, 'Mucor') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:mucor}},   1, 0x0200E800) /* Setup */
     , ({{wcid:mucor}},   2, 0x0900E800) /* MotionTable */
     , ({{wcid:mucor}},   4, 0x3000E800) /* CombatTable */
     , ({{wcid:mucor}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:mucor}}, 1, 40, 0, 0)
     , ({{wcid:mucor}}, 2, 40, 0, 0)
     , ({{wcid:mucor}}, 3, 40, 0, 0)
     , ({{wcid:mucor}}, 4, 40, 0, 0)
     , ({{wcid:mucor}}, 5, 10, 0, 0)
     , ({{wcid:mucor}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:mucor}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:mucor}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:mucor}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:mucor}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:mucor}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:mucor}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:mucor}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:mucor}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:mucor}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:mucor}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:mucor}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:mucor}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:mucor}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:mucor}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:mucor}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
