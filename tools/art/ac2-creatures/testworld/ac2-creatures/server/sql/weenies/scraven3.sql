/* Scraven (look 3): an AC2 Scraven as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:scraven3}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:scraven3}}, 'ac2-scraven3', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven3}},   1,         16) /* ItemType - Creature */
     , ({{wcid:scraven3}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:scraven3}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:scraven3}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:scraven3}},  16,          1) /* ItemUseable - No */
     , ({{wcid:scraven3}},  25,          5) /* Level */
     , ({{wcid:scraven3}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:scraven3}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:scraven3}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:scraven3}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:scraven3}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:scraven3}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven3}},   1, True ) /* Stuck */
     , ({{wcid:scraven3}},  11, False) /* IgnoreCollisions */
     , ({{wcid:scraven3}},  12, True ) /* ReportCollisions */
     , ({{wcid:scraven3}},  13, False) /* Ethereal */
     , ({{wcid:scraven3}},  14, True ) /* GravityStatus */
     , ({{wcid:scraven3}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven3}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:scraven3}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:scraven3}},   3,    0.5) /* HealthRate */
     , ({{wcid:scraven3}},   4,      5) /* StaminaRate */
     , ({{wcid:scraven3}},   5,      1) /* ManaRate */
     , ({{wcid:scraven3}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:scraven3}},  34,      1) /* PowerupTime */
     , ({{wcid:scraven3}},  36,      1) /* ChargeSpeed */
     , ({{wcid:scraven3}},  39,    1.0) /* DefaultScale */
     , ({{wcid:scraven3}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:scraven3}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:scraven3}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:scraven3}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:scraven3}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:scraven3}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:scraven3}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:scraven3}},  64,      1) /* ResistSlash */
     , ({{wcid:scraven3}},  65,      1) /* ResistPierce */
     , ({{wcid:scraven3}},  66,      1) /* ResistBludgeon */
     , ({{wcid:scraven3}},  67,      1) /* ResistFire */
     , ({{wcid:scraven3}},  68,      1) /* ResistCold */
     , ({{wcid:scraven3}},  69,      1) /* ResistAcid */
     , ({{wcid:scraven3}},  70,      1) /* ResistElectric */
     , ({{wcid:scraven3}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:scraven3}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:scraven3}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:scraven3}},  74,      1) /* ResistManaDrain */
     , ({{wcid:scraven3}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven3}}, 1, 'Scraven (look 3)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven3}},   1, 0x0200E900) /* Setup */
     , ({{wcid:scraven3}},   2, 0x0900E900) /* MotionTable */
     , ({{wcid:scraven3}},   4, 0x3000E900) /* CombatTable */
     , ({{wcid:scraven3}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:scraven3}}, 0, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 1, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 2, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 3, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 4, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 5, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 6, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 7, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 8, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 9, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 10, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 11, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 12, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 13, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 14, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 15, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 16, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 17, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 18, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 19, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 20, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 21, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 22, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 23, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 24, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 25, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 26, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 27, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 28, 0x0500E900, 0x0500E904)
     , ({{wcid:scraven3}}, 29, 0x0500E900, 0x0500E904);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:scraven3}}, 1, 40, 0, 0)
     , ({{wcid:scraven3}}, 2, 40, 0, 0)
     , ({{wcid:scraven3}}, 3, 40, 0, 0)
     , ({{wcid:scraven3}}, 4, 40, 0, 0)
     , ({{wcid:scraven3}}, 5, 10, 0, 0)
     , ({{wcid:scraven3}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:scraven3}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:scraven3}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:scraven3}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:scraven3}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:scraven3}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:scraven3}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:scraven3}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:scraven3}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:scraven3}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:scraven3}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:scraven3}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:scraven3}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:scraven3}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:scraven3}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:scraven3}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
