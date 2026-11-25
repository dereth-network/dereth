/* Scraven (look 4): an AC2 Scraven as an Asheron's Call creature, level 5, peaceful until struck. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:scraven4}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:scraven4}}, 'ac2-scraven4', 10, '2026-11-24 00:00:00') /* Creature */;

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven4}},   1,         16) /* ItemType - Creature */
     , ({{wcid:scraven4}},   2,         40) /* CreatureType - Unknown */
     , ({{wcid:scraven4}},   6,         -1) /* ItemsCapacity */
     , ({{wcid:scraven4}},   7,         -1) /* ContainersCapacity */
     , ({{wcid:scraven4}},  16,          1) /* ItemUseable - No */
     , ({{wcid:scraven4}},  25,          5) /* Level */
     , ({{wcid:scraven4}},  68,          3) /* TargetingTactic - Random, Focused */
     , ({{wcid:scraven4}},  67,         72) /* Tolerance - Provoke, Retaliate */
     , ({{wcid:scraven4}},  93,       1032) /* PhysicsState - ReportCollisions, Gravity */
     , ({{wcid:scraven4}}, 101,          1) /* AiAllowedCombatStyle - Unarmed */
     , ({{wcid:scraven4}}, 133,          2) /* ShowableOnRadar - ShowMovement */
     , ({{wcid:scraven4}}, 146,         50) /* XpOverride */;

INSERT INTO `weenie_properties_bool` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven4}},   1, True ) /* Stuck */
     , ({{wcid:scraven4}},  11, False) /* IgnoreCollisions */
     , ({{wcid:scraven4}},  12, True ) /* ReportCollisions */
     , ({{wcid:scraven4}},  13, False) /* Ethereal */
     , ({{wcid:scraven4}},  14, True ) /* GravityStatus */
     , ({{wcid:scraven4}},  19, True ) /* Attackable */;

INSERT INTO `weenie_properties_float` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven4}},   1,      5) /* HeartbeatInterval */
     , ({{wcid:scraven4}},   2,      0) /* HeartbeatTimestamp */
     , ({{wcid:scraven4}},   3,    0.5) /* HealthRate */
     , ({{wcid:scraven4}},   4,      5) /* StaminaRate */
     , ({{wcid:scraven4}},   5,      1) /* ManaRate */
     , ({{wcid:scraven4}},  31,      8) /* VisualAwarenessRange */
     , ({{wcid:scraven4}},  34,      1) /* PowerupTime */
     , ({{wcid:scraven4}},  36,      1) /* ChargeSpeed */
     , ({{wcid:scraven4}},  39,    1.0) /* DefaultScale */
     , ({{wcid:scraven4}},  13,      1) /* ArmorModVsSlash */
     , ({{wcid:scraven4}},  14,      1) /* ArmorModVsPierce */
     , ({{wcid:scraven4}},  15,      1) /* ArmorModVsBludgeon */
     , ({{wcid:scraven4}},  16,      1) /* ArmorModVsCold */
     , ({{wcid:scraven4}},  17,      1) /* ArmorModVsFire */
     , ({{wcid:scraven4}},  18,      1) /* ArmorModVsAcid */
     , ({{wcid:scraven4}},  19,      1) /* ArmorModVsElectric */
     , ({{wcid:scraven4}},  64,      1) /* ResistSlash */
     , ({{wcid:scraven4}},  65,      1) /* ResistPierce */
     , ({{wcid:scraven4}},  66,      1) /* ResistBludgeon */
     , ({{wcid:scraven4}},  67,      1) /* ResistFire */
     , ({{wcid:scraven4}},  68,      1) /* ResistCold */
     , ({{wcid:scraven4}},  69,      1) /* ResistAcid */
     , ({{wcid:scraven4}},  70,      1) /* ResistElectric */
     , ({{wcid:scraven4}},  71,      1) /* ResistHealthBoost */
     , ({{wcid:scraven4}},  72,      1) /* ResistStaminaDrain */
     , ({{wcid:scraven4}},  73,      1) /* ResistStaminaBoost */
     , ({{wcid:scraven4}},  74,      1) /* ResistManaDrain */
     , ({{wcid:scraven4}},  75,      1) /* ResistManaBoost */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven4}}, 1, 'Scraven (look 4)') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:scraven4}},   1, 0x0200E900) /* Setup */
     , ({{wcid:scraven4}},   2, 0x0900E900) /* MotionTable */
     , ({{wcid:scraven4}},   4, 0x3000E900) /* CombatTable */
     , ({{wcid:scraven4}},   8, 0x06001035) /* Icon */;

/* This look's pictures, in place of the body's own. */
INSERT INTO `weenie_properties_texture_map` (`object_Id`, `index`, `old_Id`, `new_Id`)
VALUES ({{wcid:scraven4}}, 0, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 0, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 1, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 2, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 3, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 4, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 5, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 6, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 7, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 8, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 9, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 10, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 11, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 12, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 12, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 13, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 13, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 14, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 15, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 16, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 16, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 17, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 18, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 19, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 19, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 20, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 21, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 22, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 22, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 23, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 24, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 25, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 26, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 27, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 28, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 28, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 29, 0x0500E900, 0x0500E905)
     , ({{wcid:scraven4}}, 30, 0x0500E901, 0x0500E906)
     , ({{wcid:scraven4}}, 31, 0x0500E901, 0x0500E906);

INSERT INTO `weenie_properties_attribute` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`)
VALUES ({{wcid:scraven4}}, 1, 40, 0, 0)
     , ({{wcid:scraven4}}, 2, 40, 0, 0)
     , ({{wcid:scraven4}}, 3, 40, 0, 0)
     , ({{wcid:scraven4}}, 4, 40, 0, 0)
     , ({{wcid:scraven4}}, 5, 10, 0, 0)
     , ({{wcid:scraven4}}, 6, 10, 0, 0);

INSERT INTO `weenie_properties_attribute_2nd` (`object_Id`, `type`, `init_Level`, `level_From_C_P`, `c_P_Spent`, `current_Level`)
VALUES ({{wcid:scraven4}}, 1, 20, 0, 0, 40) /* MaxHealth */
     , ({{wcid:scraven4}}, 3, 40, 0, 0, 80) /* MaxStamina */
     , ({{wcid:scraven4}}, 5, 0, 0, 0, 10) /* MaxMana */;

INSERT INTO `weenie_properties_skill` (`object_Id`, `type`, `level_From_P_P`, `s_a_c`, `p_p`, `init_Level`, `resistance_At_Last_Check`, `last_Used_Time`)
VALUES ({{wcid:scraven4}},  6, 0, 3, 0, 10, 0, 0) /* MeleeDefense */
     , ({{wcid:scraven4}},  7, 0, 3, 0, 10, 0, 0) /* MissileDefense */
     , ({{wcid:scraven4}}, 15, 0, 3, 0, 10, 0, 0) /* MagicDefense */
     , ({{wcid:scraven4}}, 24, 0, 3, 0, 10, 0, 0) /* Run */
     , ({{wcid:scraven4}}, 44, 0, 3, 0, 20, 0, 0) /* HeavyWeapons */
     , ({{wcid:scraven4}}, 45, 0, 3, 0, 20, 0, 0) /* LightWeapons */
     , ({{wcid:scraven4}}, 46, 0, 3, 0, 20, 0, 0) /* FinesseWeapons */
     , ({{wcid:scraven4}}, 52, 0, 3, 0, 20, 0, 0) /* UnarmedCombat */;

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`, `d_Var`, `base_Armor`, `armor_Vs_Slash`, `armor_Vs_Pierce`, `armor_Vs_Bludgeon`, `armor_Vs_Cold`, `armor_Vs_Fire`, `armor_Vs_Acid`, `armor_Vs_Electric`, `armor_Vs_Nether`, `b_h`, `h_l_f`, `m_l_f`, `l_l_f`, `h_r_f`, `m_r_f`, `l_r_f`, `h_l_b`, `m_l_b`, `l_l_b`, `h_r_b`, `m_r_b`, `l_r_b`)
VALUES ({{wcid:scraven4}},  0, 4, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 1, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0)
     , ({{wcid:scraven4}},  1, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0, 0.6, 0.5, 0)
     , ({{wcid:scraven4}},  2, 4, 0, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 3, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6, 0, 0.5, 0.6)
     , ({{wcid:scraven4}}, 20, 1, 3, 0.5, 5, 5, 5, 5, 5, 5, 5, 5, 0, 2, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4, 0, 0, 0.4);
