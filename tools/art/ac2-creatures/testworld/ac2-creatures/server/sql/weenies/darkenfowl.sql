/* Darkenfowl: an AC2 Darkenfowl as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:darkenfowl}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:darkenfowl}}, 'ac2-darkenfowl', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl}},   1,         16) /* ItemType - Creature */
     , ({{wcid:darkenfowl}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:darkenfowl}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:darkenfowl}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:darkenfowl}},  16,          1) /* ItemUseable - No */
     , ({{wcid:darkenfowl}},  25,          5) /* Level */
     , ({{wcid:darkenfowl}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:darkenfowl}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:darkenfowl}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:darkenfowl}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:darkenfowl}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:darkenfowl}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl}},   1, True ) /* Stuck */
     , ({{wcid:darkenfowl}},  11, False) /* IgnoreCollisions */
     , ({{wcid:darkenfowl}},  12, True ) /* ReportCollisions */
     , ({{wcid:darkenfowl}},  13, False) /* Ethereal */
     , ({{wcid:darkenfowl}},  14, True ) /* GravityStatus */
     , ({{wcid:darkenfowl}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:darkenfowl}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:darkenfowl}},   3,    0.5) /* HealthRate */
     , ({{wcid:darkenfowl}},   4,      5) /* StaminaRate */
     , ({{wcid:darkenfowl}},   5,      1) /* ManaRate */
     , ({{wcid:darkenfowl}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:darkenfowl}},  34,      1) /* PowerupTime */
     , ({{wcid:darkenfowl}},  36,      1) /* ChargeSpeed */
     , ({{wcid:darkenfowl}},  39,    1.2) /* DefaultScale */
     , ({{wcid:darkenfowl}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:darkenfowl}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:darkenfowl}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:darkenfowl}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:darkenfowl}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:darkenfowl}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:darkenfowl}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:darkenfowl}},  64,      1) /* ResistSlash */
     , ({{wcid:darkenfowl}},  65,      1) /* ResistPierce */
     , ({{wcid:darkenfowl}},  66,      1) /* ResistBludgeon */
     , ({{wcid:darkenfowl}},  67,      1) /* ResistFire */
     , ({{wcid:darkenfowl}},  68,      1) /* ResistCold */
     , ({{wcid:darkenfowl}},  69,      1) /* ResistAcid */
     , ({{wcid:darkenfowl}},  70,      1) /* ResistElectric */
     , ({{wcid:darkenfowl}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:darkenfowl}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:darkenfowl}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:darkenfowl}},  74,      1) /* ResistManaDrain */
     , ({{wcid:darkenfowl}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl}}, 1, 'Darkenfowl') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:darkenfowl}},   1, 0x0200E300) /* Setup */
     , ({{wcid:darkenfowl}},   2, 0x0900E300) /* MotionTable */
     , ({{wcid:darkenfowl}},   4, 0x3000E300) /* CombatTable */
     , ({{wcid:darkenfowl}},   8, 0x06001035) /* Icon */;

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:darkenfowl}}, 1, 40, 0, 0)
     , ({{wcid:darkenfowl}}, 2, 40, 0, 0)
     , ({{wcid:darkenfowl}}, 3, 40, 0, 0)
     , ({{wcid:darkenfowl}}, 4, 40, 0, 0)
     , ({{wcid:darkenfowl}}, 5, 10, 0, 0)
     , ({{wcid:darkenfowl}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:darkenfowl}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:darkenfowl}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:darkenfowl}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:darkenfowl}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:darkenfowl}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:darkenfowl}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:darkenfowl}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:darkenfowl}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:darkenfowl}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:darkenfowl}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:darkenfowl}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:darkenfowl}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:darkenfowl}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:darkenfowl}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:darkenfowl}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
