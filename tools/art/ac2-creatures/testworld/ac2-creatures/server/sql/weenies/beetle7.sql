/* Beetle (look 7): an AC2 Beetle as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:beetle7}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:beetle7}}, 'ac2-beetle7', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle7}},   1,         16) /* ItemType - Creature */
     , ({{wcid:beetle7}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:beetle7}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:beetle7}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:beetle7}},  16,          1) /* ItemUseable - No */
     , ({{wcid:beetle7}},  25,          5) /* Level */
     , ({{wcid:beetle7}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:beetle7}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:beetle7}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:beetle7}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:beetle7}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:beetle7}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle7}},   1, True ) /* Stuck */
     , ({{wcid:beetle7}},  11, False) /* IgnoreCollisions */
     , ({{wcid:beetle7}},  12, True ) /* ReportCollisions */
     , ({{wcid:beetle7}},  13, False) /* Ethereal */
     , ({{wcid:beetle7}},  14, True ) /* GravityStatus */
     , ({{wcid:beetle7}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle7}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:beetle7}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:beetle7}},   3,    0.5) /* HealthRate */
     , ({{wcid:beetle7}},   4,      5) /* StaminaRate */
     , ({{wcid:beetle7}},   5,      1) /* ManaRate */
     , ({{wcid:beetle7}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:beetle7}},  34,      1) /* PowerupTime */
     , ({{wcid:beetle7}},  36,      1) /* ChargeSpeed */
     , ({{wcid:beetle7}},  39,    1.0) /* DefaultScale */
     , ({{wcid:beetle7}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:beetle7}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:beetle7}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:beetle7}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:beetle7}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:beetle7}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:beetle7}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:beetle7}},  64,      1) /* ResistSlash */
     , ({{wcid:beetle7}},  65,      1) /* ResistPierce */
     , ({{wcid:beetle7}},  66,      1) /* ResistBludgeon */
     , ({{wcid:beetle7}},  67,      1) /* ResistFire */
     , ({{wcid:beetle7}},  68,      1) /* ResistCold */
     , ({{wcid:beetle7}},  69,      1) /* ResistAcid */
     , ({{wcid:beetle7}},  70,      1) /* ResistElectric */
     , ({{wcid:beetle7}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:beetle7}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:beetle7}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:beetle7}},  74,      1) /* ResistManaDrain */
     , ({{wcid:beetle7}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle7}}, 1, 'Beetle (look 7)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:beetle7}},   1, 0x0200E100) /* Setup */
     , ({{wcid:beetle7}},   2, 0x0900E100) /* MotionTable */
     , ({{wcid:beetle7}},   4, 0x3000E100) /* CombatTable */
     , ({{wcid:beetle7}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:beetle7}}, 0, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 1, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 2, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 3, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 4, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 5, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 6, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 7, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 8, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 9, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 10, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 11, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 12, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 13, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 14, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 15, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 16, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 17, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 18, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 19, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 20, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 21, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 22, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 23, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 24, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 25, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 26, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 27, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 28, 0x0500E100, 0x0500E106)
     , ({{wcid:beetle7}}, 29, 0x0500E100, 0x0500E106);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:beetle7}}, 1, 40, 0, 0)
     , ({{wcid:beetle7}}, 2, 40, 0, 0)
     , ({{wcid:beetle7}}, 3, 40, 0, 0)
     , ({{wcid:beetle7}}, 4, 40, 0, 0)
     , ({{wcid:beetle7}}, 5, 10, 0, 0)
     , ({{wcid:beetle7}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:beetle7}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:beetle7}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:beetle7}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:beetle7}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:beetle7}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:beetle7}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:beetle7}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:beetle7}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:beetle7}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:beetle7}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:beetle7}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:beetle7}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:beetle7}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:beetle7}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:beetle7}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
